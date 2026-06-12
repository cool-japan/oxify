//! API request handlers

pub mod analytics_handlers;
pub mod template_vector_handlers;
pub mod webhook_approval_handlers;
pub mod workflow_handlers;

// Re-export all types
pub use analytics_handlers::*;
pub use template_vector_handlers::*;
pub use webhook_approval_handlers::*;
pub use workflow_handlers::*;

#[cfg(test)]
mod tests;
