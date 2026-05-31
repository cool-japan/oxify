//! # oxify-connect-graphql
//!
//! A generic GraphQL client connector for OxiFY. Execute queries and mutations
//! against any GraphQL endpoint with configurable authentication, retry logic,
//! and optional introspection support.
//!
//! All providers implement the [`GraphQlExecutor`] trait for transparent substitution.
//!
//! ## Quick-start
//!
//! ```rust,no_run
//! use oxify_connect_graphql::{HttpGraphQlProvider, GraphQlConfig, GraphQlRequest, AuthConfig};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let provider = HttpGraphQlProvider::new(GraphQlConfig {
//!     endpoint: "https://api.example.com/graphql".to_string(),
//!     auth: AuthConfig::bearer("my-token"),
//!     ..GraphQlConfig::default()
//! }).expect("build provider");
//!
//! // provider.execute(GraphQlRequest::new("query { me { id name } }")).await
//! # }
//! ```

pub mod errors;
pub mod providers;
pub mod types;

pub use errors::{GraphQlError, Result};
pub use providers::{GraphQlConfig, GraphQlExecutor, HttpGraphQlProvider};
pub use types::{AuthConfig, GraphQlRequest, GraphQlResponseError, RetryConfig};
