//! Workflow testing command
//!
//! Run workflow tests from test suite files

use anyhow::{Context, Result};
use oxify_engine::{TestReport, TestSuite, WorkflowTestRunner};
use oxify_model::Workflow;
use std::fs;
use std::path::Path;

/// Run workflow tests from a test suite file
pub async fn handle_test_command(
    workflow_file: String,
    test_file: String,
    verbose: bool,
    timeout: Option<u64>,
) -> Result<()> {
    // Load workflow
    let workflow_path = Path::new(&workflow_file);
    if !workflow_path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_file);
    }

    let workflow_content = fs::read_to_string(workflow_path)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    let workflow: Workflow = match workflow_path.extension().and_then(|s| s.to_str()) {
        Some("json") => serde_json::from_str(&workflow_content)
            .with_context(|| "Failed to parse workflow JSON")?,
        Some("yaml") | Some("yml") => serde_yaml::from_str(&workflow_content)
            .with_context(|| "Failed to parse workflow YAML")?,
        _ => serde_json::from_str(&workflow_content)
            .or_else(|_| serde_yaml::from_str(&workflow_content))
            .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?,
    };

    // Validate workflow
    workflow
        .validate()
        .map_err(|error| anyhow::anyhow!("Workflow validation failed: {}", error))?;

    // Load test suite
    let test_path = Path::new(&test_file);
    if !test_path.exists() {
        anyhow::bail!("Test file not found: {}", test_file);
    }

    let test_content = fs::read_to_string(test_path)
        .with_context(|| format!("Failed to read test file: {}", test_file))?;

    let test_suite: TestSuite = match test_path.extension().and_then(|s| s.to_str()) {
        Some("json") => {
            serde_json::from_str(&test_content).with_context(|| "Failed to parse test JSON")?
        }
        Some("yaml") | Some("yml") => {
            serde_yaml::from_str(&test_content).with_context(|| "Failed to parse test YAML")?
        }
        _ => serde_json::from_str(&test_content)
            .or_else(|_| serde_yaml::from_str(&test_content))
            .with_context(|| "Failed to parse test suite (tried both JSON and YAML)")?,
    };

    println!("Running test suite: {}", test_suite.name);
    println!("  Workflow: {}", workflow.metadata.name);
    println!("  Tests: {}", test_suite.tests.len());
    println!();

    // Create test runner
    let runner = if let Some(timeout_ms) = timeout {
        WorkflowTestRunner::with_timeout(timeout_ms)
    } else {
        WorkflowTestRunner::new()
    };

    // Run tests
    let report = runner.run_suite(&workflow, &test_suite).await;

    // Display results
    display_test_report(&report, verbose)?;

    // Exit with error code if tests failed
    if report.failed_tests() > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// Display test report to console
fn display_test_report(report: &TestReport, verbose: bool) -> Result<()> {
    println!("Test Results");
    println!("============\n");

    for result in &report.results {
        let status_symbol = if result.passed { "✓" } else { "✗" };
        let status_color = if result.passed {
            "\x1b[32m"
        } else {
            "\x1b[31m"
        }; // Green/Red
        let reset_color = "\x1b[0m";

        println!(
            "{}{} {}{}",
            status_color, status_symbol, result.test_name, reset_color
        );

        if verbose || !result.passed {
            println!("  Duration: {}ms", result.execution_time_ms);
            println!(
                "  Assertions: {}/{} passed",
                result.passed_assertions(),
                result.assertions.len()
            );

            // Show failed assertions
            if !result.passed {
                println!("  Failures:");
                for assertion in &result.assertions {
                    if !assertion.passed {
                        println!("    - {}", assertion.variable);
                        println!("      Expected: {}", assertion.expected.describe());
                        println!(
                            "      Got: {}",
                            serde_json::to_string(&assertion.actual)
                                .unwrap_or_else(|_| "?".to_string())
                        );
                        if let Some(msg) = &assertion.message {
                            println!("      Message: {}", msg);
                        }
                    }
                }
            }

            // Show error if present
            if let Some(error) = &result.error {
                println!("  Error: {}", error);
            }

            println!();
        }
    }

    // Summary
    let summary_color = if report.failed_tests() == 0 {
        "\x1b[32m"
    } else {
        "\x1b[31m"
    };
    let reset_color = "\x1b[0m";

    println!("Summary");
    println!("-------");
    println!(
        "{}Tests: {}/{} passed ({:.1}%){}",
        summary_color,
        report.passed_tests(),
        report.results.len(),
        report.pass_rate() * 100.0,
        reset_color
    );
    println!("Total time: {}ms", report.total_time_ms);

    Ok(())
}

/// Run a single test case
pub async fn handle_test_single_command(
    workflow_file: String,
    test_name: String,
    input_vars: Vec<String>,
    expected_output: Option<String>,
    timeout: Option<u64>,
) -> Result<()> {
    use oxify_engine::{ExpectedStatus, ExpectedValue, WorkflowTestCase};
    use serde_json::Value;
    use std::collections::HashMap;

    // Load workflow
    let workflow_path = Path::new(&workflow_file);
    if !workflow_path.exists() {
        anyhow::bail!("Workflow file not found: {}", workflow_file);
    }

    let workflow_content = fs::read_to_string(workflow_path)
        .with_context(|| format!("Failed to read workflow file: {}", workflow_file))?;

    let workflow: Workflow = match workflow_path.extension().and_then(|s| s.to_str()) {
        Some("json") => serde_json::from_str(&workflow_content)
            .with_context(|| "Failed to parse workflow JSON")?,
        Some("yaml") | Some("yml") => serde_yaml::from_str(&workflow_content)
            .with_context(|| "Failed to parse workflow YAML")?,
        _ => serde_json::from_str(&workflow_content)
            .or_else(|_| serde_yaml::from_str(&workflow_content))
            .with_context(|| "Failed to parse workflow (tried both JSON and YAML)")?,
    };

    // Parse input variables
    let mut inputs = HashMap::new();
    for var in input_vars {
        let parts: Vec<&str> = var.splitn(2, '=').collect();
        if parts.len() == 2 {
            let key = parts[0].to_string();
            let value: Value = serde_json::from_str(parts[1])
                .unwrap_or_else(|_| Value::String(parts[1].to_string()));
            inputs.insert(key, value);
        }
    }

    // Create expected outputs
    let mut expected_outputs = HashMap::new();
    if let Some(expected) = expected_output {
        let parts: Vec<&str> = expected.splitn(2, '=').collect();
        if parts.len() == 2 {
            let key = parts[0].to_string();
            let value: Value = serde_json::from_str(parts[1])
                .unwrap_or_else(|_| Value::String(parts[1].to_string()));
            expected_outputs.insert(key, ExpectedValue::Exact { value });
        }
    }

    // Create test case
    let test_case = WorkflowTestCase {
        name: test_name.clone(),
        description: Some("Ad-hoc test from CLI".to_string()),
        inputs,
        expected_outputs,
        expected_status: ExpectedStatus::Success,
        timeout_ms: timeout,
    };

    println!("Running test: {}", test_name);
    println!("  Workflow: {}", workflow.metadata.name);
    println!();

    // Create test runner and run test
    let runner = if let Some(timeout_ms) = timeout {
        WorkflowTestRunner::with_timeout(timeout_ms)
    } else {
        WorkflowTestRunner::new()
    };

    let result = runner.run_test(&workflow, &test_case).await;

    // Display result
    let status_symbol = if result.passed { "✓" } else { "✗" };
    let status_color = if result.passed {
        "\x1b[32m"
    } else {
        "\x1b[31m"
    };
    let reset_color = "\x1b[0m";

    println!(
        "{}{} Test {}{}",
        status_color, status_symbol, result.test_name, reset_color
    );
    println!("  Duration: {}ms", result.execution_time_ms);
    println!(
        "  Assertions: {}/{} passed",
        result.passed_assertions(),
        result.assertions.len()
    );

    if !result.passed {
        println!("\nFailures:");
        for assertion in &result.assertions {
            if !assertion.passed {
                println!("  - {}", assertion.variable);
                println!("    Expected: {}", assertion.expected.describe());
                println!(
                    "    Got: {}",
                    serde_json::to_string(&assertion.actual).unwrap_or_else(|_| "?".to_string())
                );
                if let Some(msg) = &assertion.message {
                    println!("    Message: {}", msg);
                }
            }
        }
    }

    if let Some(error) = &result.error {
        println!("\nError: {}", error);
    }

    // Exit with error code if test failed
    if !result.passed {
        std::process::exit(1);
    }

    Ok(())
}
