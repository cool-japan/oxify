//! Python bindings for oxify-model using PyO3
//!
//! This module provides Python-friendly wrappers around the core Rust types,
//! enabling Python developers to create and manipulate workflows programmatically.
//!
//! # Example (Python)
//! ```python
//! from oxify_model import PyWorkflow, PyWorkflowBuilder
//!
//! # Using the builder pattern
//! workflow = (PyWorkflowBuilder("My Workflow")
//!     .description("A test workflow")
//!     .start("start")
//!     .llm("generate", "openai", "gpt-4", "Generate text: {{input}}")
//!     .end("end")
//!     .build())
//!
//! # Serialize to JSON
//! json_str = workflow.to_json()
//! ```

use crate::{
    ApprovalConfig, FormConfig, FormField, FormFieldType, LlmConfig, LoopConfig, LoopType,
    ParallelConfig, ParallelStrategy, ParallelTask, ScriptConfig, SubWorkflowConfig, SwitchCase,
    SwitchConfig, TryCatchConfig, VectorConfig, Workflow, WorkflowBuilder,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use std::collections::HashMap;

/// Python wrapper for Workflow
#[pyclass(name = "PyWorkflow", from_py_object)]
#[derive(Clone)]
pub struct PyWorkflow {
    inner: Workflow,
}

#[pymethods]
impl PyWorkflow {
    /// Create a new workflow
    #[new]
    pub fn new(name: String) -> Self {
        Self {
            inner: Workflow::new(name),
        }
    }

    /// Get workflow ID
    pub fn get_id(&self) -> String {
        self.inner.metadata.id.to_string()
    }

    /// Get workflow name
    pub fn get_name(&self) -> String {
        self.inner.metadata.name.clone()
    }

    /// Get workflow description
    pub fn get_description(&self) -> Option<String> {
        self.inner.metadata.description.clone()
    }

    /// Set workflow name
    pub fn set_name(&mut self, name: String) {
        self.inner.metadata.name = name;
        self.inner.metadata.updated_at = chrono::Utc::now();
    }

    /// Set workflow description
    pub fn set_description(&mut self, description: String) {
        self.inner.metadata.description = Some(description);
        self.inner.metadata.updated_at = chrono::Utc::now();
    }

    /// Add a tag to the workflow
    pub fn add_tag(&mut self, tag: String) {
        if !self.inner.metadata.tags.contains(&tag) {
            self.inner.metadata.tags.push(tag);
            self.inner.metadata.updated_at = chrono::Utc::now();
        }
    }

    /// Get all tags
    pub fn get_tags(&self) -> Vec<String> {
        self.inner.metadata.tags.clone()
    }

    /// Get number of nodes
    pub fn node_count(&self) -> usize {
        self.inner.nodes.len()
    }

    /// Get number of edges
    pub fn edge_count(&self) -> usize {
        self.inner.edges.len()
    }

    /// Get list of node IDs
    pub fn get_node_ids(&self) -> Vec<String> {
        self.inner.nodes.iter().map(|n| n.id.to_string()).collect()
    }

    /// Get list of node names
    pub fn get_node_names(&self) -> Vec<String> {
        self.inner.nodes.iter().map(|n| n.name.clone()).collect()
    }

    /// Validate the workflow
    pub fn validate(&self) -> PyResult<bool> {
        use crate::validation::WorkflowValidator;
        let report = WorkflowValidator::validate(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("Validation error: {:?}", e)))?;
        Ok(report.valid)
    }

    /// Convert workflow to JSON string
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string_pretty(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to serialize to JSON: {}", e)))
    }

    /// Create workflow from JSON string
    #[staticmethod]
    pub fn from_json(json_str: &str) -> PyResult<Self> {
        let workflow: Workflow = serde_json::from_str(json_str)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse JSON: {}", e)))?;
        Ok(Self { inner: workflow })
    }

    /// Convert workflow to YAML string
    pub fn to_yaml(&self) -> PyResult<String> {
        use crate::yaml::workflow_to_yaml;
        workflow_to_yaml(&self.inner)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to serialize to YAML: {}", e)))
    }

    /// Create workflow from YAML string
    #[staticmethod]
    pub fn from_yaml(yaml_str: &str) -> PyResult<Self> {
        use crate::yaml::workflow_from_yaml;
        let workflow = workflow_from_yaml(yaml_str)
            .map_err(|e| PyValueError::new_err(format!("Failed to parse YAML: {}", e)))?;
        Ok(Self { inner: workflow })
    }

    /// String representation
    pub fn __repr__(&self) -> String {
        format!(
            "PyWorkflow(id='{}', name='{}', nodes={}, edges={})",
            self.inner.metadata.id,
            self.inner.metadata.name,
            self.inner.nodes.len(),
            self.inner.edges.len()
        )
    }

    /// String representation for display
    pub fn __str__(&self) -> String {
        format!("{} ({})", self.inner.metadata.name, self.inner.metadata.id)
    }
}

/// Python wrapper for WorkflowBuilder
#[pyclass(name = "PyWorkflowBuilder")]
pub struct PyWorkflowBuilder {
    inner: Option<WorkflowBuilder>,
}

#[pymethods]
impl PyWorkflowBuilder {
    /// Create a new workflow builder
    #[new]
    pub fn new(name: &str) -> Self {
        Self {
            inner: Some(WorkflowBuilder::new(name)),
        }
    }

    /// Set workflow description
    pub fn description<'a>(mut slf: PyRefMut<'a, Self>, desc: &str) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            slf.inner = Some(builder.description(desc));
        }
        slf
    }

    /// Set workflow version
    pub fn version<'a>(mut slf: PyRefMut<'a, Self>, version: &str) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            slf.inner = Some(builder.version(version));
        }
        slf
    }

    /// Add a tag
    pub fn tag<'a>(mut slf: PyRefMut<'a, Self>, tag: &str) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            slf.inner = Some(builder.tag(tag));
        }
        slf
    }

    /// Add a start node
    pub fn start<'a>(mut slf: PyRefMut<'a, Self>, name: &str) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            slf.inner = Some(builder.start(name));
        }
        slf
    }

    /// Add an end node
    pub fn end<'a>(mut slf: PyRefMut<'a, Self>, name: &str) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            slf.inner = Some(builder.end(name));
        }
        slf
    }

    /// Add an LLM node
    #[pyo3(signature = (name, provider, model, prompt_template, system_prompt=None, temperature=None, max_tokens=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn llm<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        provider: &str,
        model: &str,
        prompt_template: &str,
        system_prompt: Option<String>,
        temperature: Option<f64>,
        max_tokens: Option<u32>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = LlmConfig {
                provider: provider.to_string(),
                model: model.to_string(),
                system_prompt,
                prompt_template: prompt_template.to_string(),
                temperature,
                max_tokens,
                tools: vec![],
                images: vec![],
                extra_params: serde_json::Value::Null,
            };
            slf.inner = Some(builder.llm(name, config));
        }
        slf
    }

    /// Add a code execution node
    #[pyo3(signature = (name, runtime, code, output, inputs=None))]
    pub fn code<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        runtime: &str,
        code: &str,
        output: &str,
        inputs: Option<Vec<String>>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = ScriptConfig {
                runtime: runtime.to_string(),
                code: code.to_string(),
                inputs: inputs.unwrap_or_default(),
                output: output.to_string(),
            };
            slf.inner = Some(builder.code(name, config));
        }
        slf
    }

    /// Add a vector retrieval node (for RAG)
    #[pyo3(signature = (name, db_type, collection, query, top_k, score_threshold=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn retriever<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        db_type: &str,
        collection: &str,
        query: &str,
        top_k: usize,
        score_threshold: Option<f64>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = VectorConfig {
                db_type: db_type.to_string(),
                collection: collection.to_string(),
                query: query.to_string(),
                top_k,
                score_threshold,
            };
            slf.inner = Some(builder.retriever(name, config));
        }
        slf
    }

    /// Add a switch/case node for multi-branch routing
    #[pyo3(signature = (name, switch_on, cases, default_case=None))]
    pub fn switch<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        switch_on: &str,
        cases: Vec<(String, String)>, // (match_value, action)
        default_case: Option<String>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let switch_cases: Vec<SwitchCase> = cases
                .into_iter()
                .map(|(match_val, action)| SwitchCase {
                    match_value: match_val,
                    action,
                })
                .collect();

            let config = SwitchConfig {
                switch_on: switch_on.to_string(),
                cases: switch_cases,
                default_case,
            };
            slf.inner = Some(builder.switch(name, config));
        }
        slf
    }

    /// Add a for-each loop node
    #[pyo3(signature = (name, collection_path, item_variable, body_expression, index_variable=None, max_iterations=None, parallel=false, max_concurrency=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn for_each<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        collection_path: &str,
        item_variable: &str,
        body_expression: &str,
        index_variable: Option<String>,
        max_iterations: Option<usize>,
        parallel: bool,
        max_concurrency: Option<usize>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = LoopConfig {
                loop_type: LoopType::ForEach {
                    collection_path: collection_path.to_string(),
                    item_variable: item_variable.to_string(),
                    index_variable,
                    body_expression: body_expression.to_string(),
                    parallel,
                    max_concurrency,
                },
                max_iterations: max_iterations.unwrap_or(1000),
            };
            slf.inner = Some(builder.loop_node(name, config));
        }
        slf
    }

    /// Add a while loop node
    #[pyo3(signature = (name, condition, body_expression, counter_variable=None, max_iterations=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn while_loop<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        condition: &str,
        body_expression: &str,
        counter_variable: Option<String>,
        max_iterations: Option<usize>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = LoopConfig {
                loop_type: LoopType::While {
                    condition: condition.to_string(),
                    body_expression: body_expression.to_string(),
                    counter_variable,
                },
                max_iterations: max_iterations.unwrap_or(1000),
            };
            slf.inner = Some(builder.loop_node(name, config));
        }
        slf
    }

    /// Add a repeat loop node (repeat N times)
    #[pyo3(signature = (name, count, body_expression, index_variable=None, max_iterations=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn repeat<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        count: &str,
        body_expression: &str,
        index_variable: Option<String>,
        max_iterations: Option<usize>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = LoopConfig {
                loop_type: LoopType::Repeat {
                    count: count.to_string(),
                    body_expression: body_expression.to_string(),
                    index_variable,
                },
                max_iterations: max_iterations.unwrap_or(1000),
            };
            slf.inner = Some(builder.loop_node(name, config));
        }
        slf
    }

    /// Add a try-catch error handling node
    #[pyo3(signature = (name, try_expression, catch_expression=None, finally_expression=None, rethrow=None, error_variable=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn try_catch<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        try_expression: &str,
        catch_expression: Option<String>,
        finally_expression: Option<String>,
        rethrow: Option<bool>,
        error_variable: Option<String>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = TryCatchConfig {
                try_expression: try_expression.to_string(),
                catch_expression,
                finally_expression,
                rethrow: rethrow.unwrap_or(false),
                error_variable: error_variable.unwrap_or_else(|| "error".to_string()),
            };
            slf.inner = Some(builder.try_catch(name, config));
        }
        slf
    }

    /// Add a sub-workflow node
    #[pyo3(signature = (name, workflow_path, input_mappings=None, output_variable=None, inherit_context=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn sub_workflow<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        workflow_path: &str,
        input_mappings: Option<Vec<(String, String)>>,
        output_variable: Option<String>,
        inherit_context: Option<bool>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let mappings: HashMap<String, String> =
                input_mappings.unwrap_or_default().into_iter().collect();

            let config = SubWorkflowConfig {
                workflow_path: workflow_path.to_string(),
                input_mappings: mappings,
                output_variable,
                inherit_context: inherit_context.unwrap_or(false),
            };
            slf.inner = Some(builder.sub_workflow(name, config));
        }
        slf
    }

    /// Add a parallel execution node
    ///
    /// # Arguments
    /// * `name` - Node name
    /// * `tasks` - List of (task_id, expression) tuples
    /// * `strategy` - "wait_all", "race", or "all_settled" (default: "wait_all")
    /// * `max_concurrency` - Maximum concurrent tasks
    /// * `timeout_ms` - Timeout in milliseconds
    #[pyo3(signature = (name, tasks, strategy=None, max_concurrency=None, timeout_ms=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn parallel<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        tasks: Vec<(String, String)>,
        strategy: Option<String>,
        max_concurrency: Option<usize>,
        timeout_ms: Option<u64>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let parallel_tasks: Vec<ParallelTask> = tasks
                .into_iter()
                .map(|(id, expr)| ParallelTask {
                    id,
                    expression: expr,
                    description: None,
                })
                .collect();

            let strat = match strategy.as_deref() {
                Some("race") => ParallelStrategy::Race,
                Some("all_settled") => ParallelStrategy::AllSettled,
                _ => ParallelStrategy::WaitAll,
            };

            let config = ParallelConfig {
                strategy: strat,
                tasks: parallel_tasks,
                max_concurrency,
                timeout_ms,
            };
            slf.inner = Some(builder.parallel(name, config));
        }
        slf
    }

    /// Add an approval node (human-in-the-loop)
    #[pyo3(signature = (name, message, description=None, approvers=None, timeout_seconds=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn approval<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        message: &str,
        description: Option<String>,
        approvers: Option<Vec<String>>,
        timeout_seconds: Option<u64>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let config = ApprovalConfig {
                message: message.to_string(),
                description,
                approvers: approvers.unwrap_or_default(),
                timeout_seconds,
                context_data: serde_json::Value::Null,
            };
            slf.inner = Some(builder.approval(name, config));
        }
        slf
    }

    /// Add a form input node (human-in-the-loop)
    ///
    /// # Arguments
    /// * `name` - Node name
    /// * `title` - Form title
    /// * `fields` - List of (field_id, label, field_type, required) tuples
    ///   - field_type: "text", "number", "email", "password", "textarea",
    ///                 "select", "multiselect", "radio", "checkbox", "date", "datetime"
    /// * `description` - Optional form description
    /// * `timeout_seconds` - Optional timeout
    /// * `allowed_submitters` - Optional list of user IDs who can submit
    #[pyo3(signature = (name, title, fields, description=None, timeout_seconds=None, allowed_submitters=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn form<'a>(
        mut slf: PyRefMut<'a, Self>,
        name: &str,
        title: &str,
        fields: Vec<(String, String, String, bool)>, // (id, label, type, required)
        description: Option<String>,
        timeout_seconds: Option<u64>,
        allowed_submitters: Option<Vec<String>>,
    ) -> PyRefMut<'a, Self> {
        if let Some(builder) = slf.inner.take() {
            let form_fields: Vec<FormField> = fields
                .into_iter()
                .map(|(id, label, field_type, required)| {
                    let ft = match field_type.to_lowercase().as_str() {
                        "number" => FormFieldType::Number,
                        "email" => FormFieldType::Email,
                        "password" => FormFieldType::Password,
                        "textarea" => FormFieldType::TextArea,
                        "select" => FormFieldType::Select,
                        "multiselect" => FormFieldType::MultiSelect,
                        "radio" => FormFieldType::Radio,
                        "checkbox" => FormFieldType::Checkbox,
                        "date" => FormFieldType::Date,
                        "datetime" => FormFieldType::DateTime,
                        _ => FormFieldType::Text,
                    };
                    FormField {
                        id,
                        label,
                        field_type: ft,
                        required,
                        default_value: None,
                        validation: None,
                        options: Vec::new(),
                    }
                })
                .collect();

            let config = FormConfig {
                title: title.to_string(),
                description,
                fields: form_fields,
                timeout_seconds,
                allowed_submitters: allowed_submitters.unwrap_or_default(),
            };
            slf.inner = Some(builder.form(name, config));
        }
        slf
    }

    /// Build the workflow
    pub fn build(mut slf: PyRefMut<'_, Self>) -> PyResult<PyWorkflow> {
        let builder = slf
            .inner
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Builder already consumed"))?;
        let workflow = builder.build();
        Ok(PyWorkflow { inner: workflow })
    }
}

/// Initialize the Python module
#[pymodule]
#[allow(dead_code)]
fn oxify_model(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyWorkflow>()?;
    m.add_class::<PyWorkflowBuilder>()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_py_workflow_creation() {
        let workflow = PyWorkflow::new("Test Workflow".to_string());
        assert_eq!(workflow.get_name(), "Test Workflow");
        assert!(workflow.get_description().is_none());
    }

    #[test]
    fn test_py_workflow_set_name() {
        let mut workflow = PyWorkflow::new("Test Workflow".to_string());
        workflow.set_name("Updated Workflow".to_string());
        assert_eq!(workflow.get_name(), "Updated Workflow");
    }

    #[test]
    fn test_py_workflow_set_description() {
        let mut workflow = PyWorkflow::new("Test Workflow".to_string());
        workflow.set_description("A test workflow".to_string());
        assert_eq!(
            workflow.get_description(),
            Some("A test workflow".to_string())
        );
    }

    #[test]
    fn test_py_workflow_tags() {
        let mut workflow = PyWorkflow::new("Test Workflow".to_string());
        workflow.add_tag("production".to_string());
        workflow.add_tag("v1".to_string());
        workflow.add_tag("production".to_string()); // Duplicate should not be added

        let tags = workflow.get_tags();
        assert_eq!(tags.len(), 2);
        assert!(tags.contains(&"production".to_string()));
        assert!(tags.contains(&"v1".to_string()));
    }

    #[test]
    fn test_py_workflow_builder() {
        // Create a minimal workflow
        let builder = WorkflowBuilder::new("Test");
        let builder = builder.description("Test workflow");
        let builder = builder.start("start");
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.get_name(), "Test");
        assert_eq!(py_workflow.node_count(), 2);
    }

    #[test]
    fn test_py_workflow_json_serialization() {
        let builder = WorkflowBuilder::new("Test");
        let builder = builder.start("start");
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        let json = py_workflow.to_json().unwrap();
        assert!(json.contains("Test"));

        let restored = PyWorkflow::from_json(&json).unwrap();
        assert_eq!(restored.get_name(), "Test");
        assert_eq!(restored.node_count(), 2);
    }

    #[test]
    fn test_py_workflow_yaml_serialization() {
        let builder = WorkflowBuilder::new("Test");
        let builder = builder.start("start");
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        let yaml = py_workflow.to_yaml().unwrap();
        assert!(yaml.contains("Test"));

        let restored = PyWorkflow::from_yaml(&yaml).unwrap();
        assert_eq!(restored.get_name(), "Test");
        assert_eq!(restored.node_count(), 2);
    }

    #[test]
    fn test_py_workflow_repr() {
        let workflow = PyWorkflow::new("Test Workflow".to_string());
        let repr = workflow.__repr__();
        assert!(repr.contains("Test Workflow"));
        assert!(repr.contains("nodes=0"));
        assert!(repr.contains("edges=0"));
    }

    #[test]
    fn test_py_workflow_node_names() {
        let builder = WorkflowBuilder::new("Test");
        let builder = builder.start("StartNode");
        let builder = builder.end("EndNode");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        let names = py_workflow.get_node_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"StartNode".to_string()));
        assert!(names.contains(&"EndNode".to_string()));
    }

    #[test]
    fn test_py_workflow_validation() {
        let builder = WorkflowBuilder::new("Test");
        let builder = builder.start("start");
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        let is_valid = py_workflow.validate().unwrap();
        assert!(is_valid);
    }

    #[test]
    fn test_py_workflow_builder_with_retriever() {
        use crate::VectorConfig;

        let builder = WorkflowBuilder::new("RAG Pipeline");
        let builder = builder.start("start");
        let config = VectorConfig {
            db_type: "qdrant".to_string(),
            collection: "documents".to_string(),
            query: "{{user_query}}".to_string(),
            top_k: 5,
            score_threshold: Some(0.7),
        };
        let builder = builder.retriever("retrieve_docs", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"retrieve_docs".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_loop() {
        use crate::{LoopConfig, LoopType};

        let builder = WorkflowBuilder::new("Loop Test");
        let builder = builder.start("start");
        let config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: None,
                body_expression: "process {{item}}".to_string(),
                parallel: false,
                max_concurrency: None,
            },
            max_iterations: 100,
        };
        let builder = builder.loop_node("loop", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
    }

    #[test]
    fn test_py_workflow_builder_with_switch() {
        use crate::{SwitchCase, SwitchConfig};

        let builder = WorkflowBuilder::new("Switch Test");
        let builder = builder.start("start");
        let config = SwitchConfig {
            switch_on: "{{status}}".to_string(),
            cases: vec![
                SwitchCase {
                    match_value: "success".to_string(),
                    action: "handle_success".to_string(),
                },
                SwitchCase {
                    match_value: "error".to_string(),
                    action: "handle_error".to_string(),
                },
            ],
            default_case: Some("handle_default".to_string()),
        };
        let builder = builder.switch("router", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow.get_node_names().contains(&"router".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_while_loop() {
        use crate::{LoopConfig, LoopType};

        let builder = WorkflowBuilder::new("While Test");
        let builder = builder.start("start");
        let config = LoopConfig {
            loop_type: LoopType::While {
                condition: "{{count}} < 10".to_string(),
                body_expression: "increment {{count}}".to_string(),
                counter_variable: Some("iteration".to_string()),
            },
            max_iterations: 100,
        };
        let builder = builder.loop_node("while_loop", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"while_loop".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_repeat() {
        use crate::{LoopConfig, LoopType};

        let builder = WorkflowBuilder::new("Repeat Test");
        let builder = builder.start("start");
        let config = LoopConfig {
            loop_type: LoopType::Repeat {
                count: "5".to_string(),
                body_expression: "process item {{index}}".to_string(),
                index_variable: Some("index".to_string()),
            },
            max_iterations: 100,
        };
        let builder = builder.loop_node("repeat_loop", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
    }

    #[test]
    fn test_py_workflow_builder_with_try_catch() {
        use crate::TryCatchConfig;

        let builder = WorkflowBuilder::new("TryCatch Test");
        let builder = builder.start("start");
        let config = TryCatchConfig {
            try_expression: "risky_operation()".to_string(),
            catch_expression: Some("handle_error({{error}})".to_string()),
            finally_expression: Some("cleanup()".to_string()),
            rethrow: false,
            error_variable: "error".to_string(),
        };
        let builder = builder.try_catch("try_catch", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"try_catch".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_parallel() {
        use crate::{ParallelConfig, ParallelStrategy, ParallelTask};

        let builder = WorkflowBuilder::new("Parallel Test");
        let builder = builder.start("start");
        let config = ParallelConfig {
            strategy: ParallelStrategy::WaitAll,
            tasks: vec![
                ParallelTask {
                    id: "task1".to_string(),
                    expression: "process_a()".to_string(),
                    description: None,
                },
                ParallelTask {
                    id: "task2".to_string(),
                    expression: "process_b()".to_string(),
                    description: None,
                },
            ],
            max_concurrency: Some(2),
            timeout_ms: Some(5000),
        };
        let builder = builder.parallel("parallel_exec", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"parallel_exec".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_approval() {
        use crate::ApprovalConfig;

        let builder = WorkflowBuilder::new("Approval Test");
        let builder = builder.start("start");
        let config = ApprovalConfig {
            message: "Please approve this action".to_string(),
            description: Some("Critical operation requiring review".to_string()),
            approvers: vec!["admin".to_string(), "manager".to_string()],
            timeout_seconds: Some(3600),
            context_data: serde_json::json!({"action": "deploy"}),
        };
        let builder = builder.approval("approval_node", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"approval_node".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_form() {
        use crate::{FormConfig, FormField, FormFieldType};

        let builder = WorkflowBuilder::new("Form Test");
        let builder = builder.start("start");
        let config = FormConfig {
            title: "User Information".to_string(),
            description: Some("Please fill in your details".to_string()),
            fields: vec![
                FormField {
                    id: "name".to_string(),
                    label: "Full Name".to_string(),
                    field_type: FormFieldType::Text,
                    required: true,
                    default_value: None,
                    validation: None,
                    options: Vec::new(),
                },
                FormField {
                    id: "email".to_string(),
                    label: "Email Address".to_string(),
                    field_type: FormFieldType::Email,
                    required: true,
                    default_value: None,
                    validation: None,
                    options: Vec::new(),
                },
                FormField {
                    id: "age".to_string(),
                    label: "Age".to_string(),
                    field_type: FormFieldType::Number,
                    required: false,
                    default_value: None,
                    validation: None,
                    options: Vec::new(),
                },
            ],
            timeout_seconds: Some(1800),
            allowed_submitters: vec!["user".to_string()],
        };
        let builder = builder.form("user_form", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"user_form".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_sub_workflow() {
        use crate::SubWorkflowConfig;
        use std::collections::HashMap;

        let builder = WorkflowBuilder::new("SubWorkflow Test");
        let builder = builder.start("start");

        let mut input_mappings = HashMap::new();
        input_mappings.insert("child_input".to_string(), "{{parent_var}}".to_string());

        let config = SubWorkflowConfig {
            workflow_path: "./child_workflow.json".to_string(),
            input_mappings,
            output_variable: Some("child_result".to_string()),
            inherit_context: false,
        };
        let builder = builder.sub_workflow("run_child", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);
        assert!(py_workflow
            .get_node_names()
            .contains(&"run_child".to_string()));
    }

    #[test]
    fn test_py_workflow_builder_with_parallel_foreach() {
        use crate::{LoopConfig, LoopType};

        let builder = WorkflowBuilder::new("Parallel ForEach Test");
        let builder = builder.start("start");
        let config = LoopConfig {
            loop_type: LoopType::ForEach {
                collection_path: "items".to_string(),
                item_variable: "item".to_string(),
                index_variable: Some("idx".to_string()),
                body_expression: "process {{item}}".to_string(),
                parallel: true,
                max_concurrency: Some(10),
            },
            max_iterations: 100,
        };
        let builder = builder.loop_node("parallel_loop", config);
        let builder = builder.end("end");
        let workflow = builder.build();

        let py_workflow = PyWorkflow { inner: workflow };
        assert_eq!(py_workflow.node_count(), 3);

        // Verify the parallel configuration
        if let Some(node) = py_workflow
            .inner
            .nodes
            .iter()
            .find(|n| n.name == "parallel_loop")
        {
            if let crate::NodeKind::Loop(config) = &node.kind {
                if let LoopType::ForEach {
                    parallel,
                    max_concurrency,
                    ..
                } = &config.loop_type
                {
                    assert!(*parallel);
                    assert_eq!(*max_concurrency, Some(10));
                } else {
                    panic!("Expected ForEach loop");
                }
            } else {
                panic!("Expected Loop node");
            }
        } else {
            panic!("parallel_loop node not found");
        }
    }
}
