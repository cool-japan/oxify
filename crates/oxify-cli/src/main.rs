use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};

mod commands;
mod config;
mod tui;

#[derive(Parser)]
#[command(name = "oxify")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "OxiFY - Workflow Automation Engine CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Workflow {
        #[command(subcommand)]
        command: commands::workflow::WorkflowCommands,
    },
    Execute {
        #[command(subcommand)]
        command: commands::execute::ExecuteCommands,
    },
    Run {
        workflow_file: String,
        #[arg(short, long)]
        vars: Vec<String>,
        #[arg(short, long)]
        output: Option<String>,
    },
    Init {
        template: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    Scaffold {
        #[arg(short, long)]
        output: Option<String>,
    },
    Visualize {
        workflow_file: String,
        #[arg(short, long, default_value = "ascii", value_parser = ["ascii", "dot"])]
        format: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    Test {
        workflow_file: String,
        test_file: Option<String>,
        #[arg(short, long)]
        verbose: bool,
        #[arg(short, long)]
        timeout: Option<u64>,
        #[arg(short = 'n', long)]
        name: Option<String>,
        #[arg(short = 'i', long)]
        input: Vec<String>,
        #[arg(short = 'e', long)]
        expect: Option<String>,
    },
    Cost {
        workflow_file: Option<String>,
        #[arg(short, long)]
        compare: Vec<String>,
        #[arg(long)]
        avg_prompt_tokens: Option<u32>,
        #[arg(long)]
        avg_response_tokens: Option<u32>,
        #[arg(short, long)]
        breakdown: bool,
    },
    Analyze {
        workflow_file: String,
        #[arg(short, long, value_parser = ["batching", "structure", "optimize"])]
        analysis_type: String,
        #[arg(long)]
        min_batch_size: Option<usize>,
        #[arg(long)]
        max_batch_size: Option<usize>,
        #[arg(long)]
        strict: bool,
    },
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    Schedule {
        #[command(subcommand)]
        command: commands::schedule::ScheduleCommands,
    },
    Secret {
        #[command(subcommand)]
        command: commands::secret::SecretCommands,
    },
    Version {
        #[command(subcommand)]
        command: commands::version::VersionCommands,
    },
    Checkpoint {
        #[command(subcommand)]
        command: commands::checkpoint::CheckpointCommands,
    },
    Webhook {
        #[command(subcommand)]
        command: commands::webhook::WebhookCommands,
    },
    Nodes {
        #[command(subcommand)]
        command: commands::nodes::NodeCommands,
    },
    Stats {
        #[command(subcommand)]
        command: commands::stats::StatsCommands,
    },
    Deploy {
        #[command(subcommand)]
        command: commands::deploy::DeployCommand,
    },
    /// Vision/OCR processing commands
    Vision {
        #[command(subcommand)]
        command: commands::vision::VisionCommands,
    },
    /// Interactive terminal UI (TUI) for monitoring workflows and executions
    Tui(commands::tui::TuiArgs),
    /// Generate a workflow from a natural language description
    Generate(commands::generate::GenerateArgs),
    /// Generate shell completion scripts
    Completion {
        /// Shell type (bash, zsh, fish, powershell, elvish)
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    Init,
    Show,
    Set { key: String, value: String },
    Unset { key: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Workflow { command } => {
            commands::workflow::handle_workflow_command(command).await
        }
        Commands::Execute { command } => commands::execute::handle_execute_command(command).await,
        Commands::Run {
            workflow_file,
            vars,
            output,
        } => commands::run::handle_run_command(workflow_file, vars, output).await,
        Commands::Init { template, output } => {
            commands::template::init_template(&template, output).await
        }
        Commands::Scaffold { output } => commands::scaffold::scaffold_interactive(output).await,
        Commands::Visualize {
            workflow_file,
            format,
            output,
        } => commands::visualize::visualize_workflow(&workflow_file, &format, output).await,
        Commands::Test {
            workflow_file,
            test_file,
            verbose,
            timeout,
            name,
            input,
            expect,
        } => {
            if let Some(test_path) = test_file {
                commands::test::handle_test_command(workflow_file, test_path, verbose, timeout)
                    .await
            } else if let Some(test_name) = name {
                commands::test::handle_test_single_command(
                    workflow_file,
                    test_name,
                    input,
                    expect,
                    timeout,
                )
                .await
            } else {
                anyhow::bail!("Either --test-file or --name must be specified")
            }
        }
        Commands::Cost {
            workflow_file,
            compare,
            avg_prompt_tokens,
            avg_response_tokens,
            breakdown,
        } => {
            if !compare.is_empty() {
                commands::cost::handle_cost_compare_command(compare).await
            } else if let Some(file) = workflow_file {
                commands::cost::handle_cost_command(
                    file,
                    avg_prompt_tokens,
                    avg_response_tokens,
                    breakdown,
                )
                .await
            } else {
                anyhow::bail!("Either workflow_file or --compare must be specified")
            }
        }
        Commands::Analyze {
            workflow_file,
            analysis_type,
            min_batch_size,
            max_batch_size,
            strict,
        } => match analysis_type.as_str() {
            "batching" => {
                commands::analyze::handle_analyze_batching_command(
                    workflow_file,
                    min_batch_size,
                    max_batch_size,
                )
                .await
            }
            "structure" => commands::analyze::handle_analyze_structure_command(workflow_file).await,
            "optimize" => {
                commands::analyze::handle_analyze_optimization_command(workflow_file, strict).await
            }
            _ => anyhow::bail!("Invalid analysis type. Use 'batching', 'structure', or 'optimize'"),
        },
        Commands::Config { command } => handle_config_command(command).await,
        Commands::Schedule { command } => {
            commands::schedule::handle_schedule_command(command).await
        }
        Commands::Secret { command } => commands::secret::handle_secret_command(command).await,
        Commands::Version { command } => commands::version::handle_version_command(command).await,
        Commands::Checkpoint { command } => {
            commands::checkpoint::handle_checkpoint_command(command).await
        }
        Commands::Webhook { command } => commands::webhook::handle_webhook_command(command).await,
        Commands::Nodes { command } => commands::nodes::handle_nodes_command(command).await,
        Commands::Stats { command } => commands::stats::handle_stats_command(command).await,
        Commands::Deploy { command } => {
            commands::deploy::execute(commands::deploy::DeployArgs { command }).await
        }
        Commands::Vision { command } => commands::vision::handle_vision_command(command).await,
        Commands::Tui(args) => commands::tui::run(args).await,
        Commands::Generate(args) => commands::generate::run(args).await,
        Commands::Completion { shell } => {
            generate_completion(shell);
            Ok(())
        }
    }
}

fn generate_completion(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
}

async fn handle_config_command(command: ConfigCommands) -> Result<()> {
    match command {
        ConfigCommands::Init => config::Config::init(),
        ConfigCommands::Show => config::Config::show(),
        ConfigCommands::Set { key, value } => config::Config::set(&key, &value),
        ConfigCommands::Unset { key } => config::Config::unset(&key),
    }
}
