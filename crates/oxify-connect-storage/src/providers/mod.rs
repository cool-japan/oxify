use async_trait::async_trait;
use bytes::Bytes;
use std::time::Duration;

use crate::{
    errors::Result,
    types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult},
};

pub mod memory;

#[cfg(feature = "aws")]
pub mod s3;

/// Abstraction over an object storage backend.
///
/// All methods are async and object-safe (via [`async_trait`]).  Providers
/// **must** be [`Send`] + [`Sync`] so they can be shared across tasks.
#[async_trait]
pub trait ObjectStoreProvider: Send + Sync {
    /// A human-readable name identifying this provider (e.g. `"s3"`, `"memory"`).
    fn provider_name(&self) -> &str;

    /// Upload `data` to `bucket/key`, attaching the supplied `meta`.
    ///
    /// Returns a [`PutResult`] that contains at minimum the written `key`.
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        meta: ObjectMeta,
    ) -> Result<PutResult>;

    /// Download the full object at `bucket/key`.
    ///
    /// Returns [`StorageError::NotFound`] when the key does not exist.
    async fn get_object(&self, bucket: &str, key: &str) -> Result<ObjectData>;

    /// Remove the object at `bucket/key`.
    ///
    /// Returns [`StorageError::NotFound`] when the key does not exist.
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()>;

    /// List at most `max` objects in `bucket` whose keys begin with `prefix`.
    ///
    /// Pass `None` for `prefix` to list all objects.  Results are sorted
    /// lexicographically by key.
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        max: usize,
    ) -> Result<Vec<ObjectListing>>;

    /// Generate a presigned URL for `bucket/key` valid for `ttl`.
    ///
    /// Not all providers support presigned URLs; implementations that do not
    /// should return [`StorageError::Unsupported`].
    async fn presigned_url(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
        op: PresignOp,
    ) -> Result<String>;
}
