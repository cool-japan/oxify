use clap::Parser;

/// Arguments for the TUI subcommand.
#[derive(Parser, Debug)]
pub struct TuiArgs {
    /// OxiFY API base URL (also read from OXIFY_API_URL environment variable at startup).
    #[arg(long, default_value = "http://localhost:8080")]
    pub api_url: String,

    /// Polling / tick interval in milliseconds.
    #[arg(long, default_value_t = 250)]
    pub tick_ms: u64,
}

pub async fn run(args: TuiArgs) -> anyhow::Result<()> {
    // Allow OXIFY_API_URL to override the CLI default without requiring the `env` clap feature.
    let api_url = std::env::var("OXIFY_API_URL").unwrap_or(args.api_url);
    crate::tui::app::TuiApp::run(api_url, args.tick_ms).await
}
