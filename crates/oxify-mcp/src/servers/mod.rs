//! Built-in MCP servers for common operations

pub mod database;
pub mod filesystem;
pub mod git;
pub mod shell;
pub mod web;
pub mod workflow;

pub use database::{
    DatabaseConfig, DatabaseServer, DatabaseType, ExecuteResult, QueryResult, StatementResult,
    TransactionResult,
};
pub use filesystem::FilesystemServer;
pub use git::GitServer;
pub use shell::ShellServer;
pub use web::WebServer;
pub use workflow::{WorkflowExecutor, WorkflowServer, WorkflowServerConfig};
