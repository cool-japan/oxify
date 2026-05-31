use async_trait::async_trait;
use serde_json::Value;

use crate::{errors::Result, types::GraphQlRequest};

pub mod http;

pub use http::{GraphQlConfig, HttpGraphQlProvider};

// ---------------------------------------------------------------------------
// GraphQlExecutor trait
// ---------------------------------------------------------------------------

/// Core trait implemented by every GraphQL client back-end.
///
/// All methods are async so that implementations can perform network I/O
/// without blocking the executor.  The trait is object-safe through
/// `async_trait`.
#[async_trait]
pub trait GraphQlExecutor: Send + Sync {
    /// A human-readable identifier for this provider (e.g. `"graphql-http"`).
    fn provider_name(&self) -> &str;

    /// Execute a GraphQL query or mutation and return the `data` portion of
    /// the response as a raw [`serde_json::Value`].
    ///
    /// Returns `Err(GraphQlError::GraphQl)` when the server reports errors in
    /// the `errors` array, even if the HTTP status was 2xx.
    async fn execute(&self, req: GraphQlRequest) -> Result<Value>;

    /// Send the standard introspection query and return the raw schema data.
    ///
    /// This method is only compiled when the `introspection` Cargo feature is
    /// enabled, keeping the default binary size minimal.
    #[cfg(feature = "introspection")]
    async fn introspect(&self) -> Result<Value>;
}
