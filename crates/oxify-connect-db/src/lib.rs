//! # oxify-connect-db
//!
//! Database connectors for OxiFY workflows — PostgreSQL, MySQL, MongoDB.
//!
//! ## Features
//! - `postgres` — PostgreSQL via sqlx
//! - `mysql` — MySQL/MariaDB via sqlx
//! - `mongodb-store` — MongoDB via the official driver
//!
//! ## Quick-start
//!
//! ```rust,no_run
//! # #[cfg(feature = "postgres")]
//! use oxify_connect_db::sql::{DbConfig, SqlExecutor, PostgresProvider};
//!
//! # #[tokio::main]
//! # async fn main() -> oxify_connect_db::Result<()> {
//! # #[cfg(feature = "postgres")] {
//! let cfg = DbConfig::from_env()?;
//! let db = PostgresProvider::new(cfg).await?;
//! let rows = db.fetch_all("SELECT id, name FROM users WHERE active = $1", &[true.into()]).await?;
//! # }
//! # Ok(())
//! # }
//! ```

pub mod document;
pub mod errors;
pub mod sql;

pub use errors::{DbError, Result};

#[cfg(feature = "postgres")]
pub use sql::postgres::PostgresProvider;

#[cfg(feature = "mysql")]
pub use sql::mysql::MySqlProvider;

#[cfg(feature = "mongodb-store")]
pub use document::mongo::MongoProvider;
