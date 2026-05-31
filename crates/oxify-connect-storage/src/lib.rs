//! Object storage connectors for OxiFY.
//!
//! This crate provides a unified [`ObjectStoreProvider`] trait and concrete
//! implementations for:
//!
//! * **In-memory** (feature `memory`, enabled by default) — ideal for unit
//!   tests and local development where you do not want real network I/O.
//! * **Amazon S3 / MinIO** (feature `aws`) — backed by the [`object_store`]
//!   crate; supports any S3-compatible endpoint.
//!
//! # Quick start
//!
//! ```rust
//! use oxify_connect_storage::{MemoryStoreProvider, ObjectStoreProvider, ObjectMeta};
//! use bytes::Bytes;
//!
//! # #[tokio::main]
//! # async fn main() -> oxify_connect_storage::Result<()> {
//! let provider = MemoryStoreProvider::new();
//! let result = provider
//!     .put_object("my-bucket", "hello.txt", Bytes::from("world"), ObjectMeta::default())
//!     .await?;
//! let obj = provider.get_object("my-bucket", "hello.txt").await?;
//! assert_eq!(obj.data, Bytes::from("world"));
//! # Ok(())
//! # }
//! ```

pub mod errors;
pub mod providers;
pub mod types;

pub use errors::{Result, StorageError};
pub use providers::memory::MemoryStoreProvider;
pub use providers::ObjectStoreProvider;
pub use types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult};

#[cfg(feature = "aws")]
pub use providers::s3::{S3Config, S3StoreProvider};

#[cfg(feature = "gcs")]
pub use providers::gcs::{GcsConfig, GcsStoreProvider};

#[cfg(feature = "azure")]
pub use providers::azure_blob::{AzureBlobConfig, AzureBlobStoreProvider};

#[cfg(feature = "local")]
pub use providers::local::{LocalFsConfig, LocalFsProvider};
