//! Built-in MCP servers for common operations

/// Bounded CSS selector engine backing the `web_scrape` tool. Private: it is an
/// implementation detail of [`web`], not part of this crate's public API.
mod css_select;

pub mod database;
pub mod filesystem;
pub mod git;
#[cfg(feature = "github")]
pub mod github;
#[cfg(feature = "github-actions")]
pub mod github_actions;
#[cfg(feature = "gitlab")]
pub mod gitlab;
#[cfg(feature = "jira")]
pub mod jira;
#[cfg(feature = "linear")]
pub mod linear;
pub mod shell;
pub mod web;
pub mod workflow;

pub use database::{
    DatabaseConfig, DatabaseServer, DatabaseType, ExecuteResult, QueryResult, StatementResult,
    TransactionResult,
};
pub use filesystem::FilesystemServer;
pub use git::GitServer;
#[cfg(feature = "github")]
pub use github::{GitHubConfig, GitHubServer};
#[cfg(feature = "github-actions")]
pub use github_actions::{GitHubActionsConfig, GitHubActionsServer};
#[cfg(feature = "gitlab")]
pub use gitlab::{GitLabConfig, GitLabServer};
#[cfg(feature = "jira")]
pub use jira::{JiraConfig, JiraServer};
#[cfg(feature = "linear")]
pub use linear::{LinearConfig, LinearServer};
pub use shell::ShellServer;
pub use web::WebServer;
pub use workflow::{WorkflowExecutor, WorkflowServer, WorkflowServerConfig};
