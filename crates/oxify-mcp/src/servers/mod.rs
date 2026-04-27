//! Built-in MCP servers for common operations

pub mod database;
pub mod filesystem;
pub mod git;
#[cfg(feature = "github")]
pub mod github;
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
pub use shell::ShellServer;
pub use web::WebServer;
pub use workflow::{WorkflowExecutor, WorkflowServer, WorkflowServerConfig};
