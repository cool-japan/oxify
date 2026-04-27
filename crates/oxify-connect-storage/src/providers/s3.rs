//! AWS S3 (and S3-compatible) object store provider.
//!
//! Enabled via the `aws` Cargo feature.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use object_store::{aws::AmazonS3Builder, path::Path as StoragePath, ObjectStore};
use tracing::{debug, instrument};

use super::ObjectStoreProvider;
use crate::{
    errors::{Result, StorageError},
    types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult},
};

// ---------------------------------------------------------------------------
// S3Config
// ---------------------------------------------------------------------------

/// Configuration for the S3 (or S3-compatible) store.
#[derive(Debug, Clone)]
pub struct S3Config {
    /// Default bucket name used when the caller does not override it.
    pub bucket: String,
    /// AWS region (e.g. `"us-east-1"`).
    pub region: String,
    /// AWS access key ID.  Falls back to environment / instance profile when `None`.
    pub access_key_id: Option<String>,
    /// AWS secret access key.  Falls back to environment / instance profile when `None`.
    pub secret_access_key: Option<String>,
    /// Custom endpoint URL for MinIO or other S3-compatible stores.
    pub endpoint: Option<String>,
}

impl S3Config {
    /// Populate a [`S3Config`] from well-known environment variables.
    ///
    /// | Variable | Field |
    /// |---|---|
    /// | `AWS_DEFAULT_REGION` | `region` (defaults to `"us-east-1"`) |
    /// | `AWS_DEFAULT_BUCKET` | `bucket` |
    /// | `AWS_ACCESS_KEY_ID` | `access_key_id` |
    /// | `AWS_SECRET_ACCESS_KEY` | `secret_access_key` |
    /// | `S3_ENDPOINT` | `endpoint` |
    pub fn from_env() -> Result<Self> {
        let region =
            std::env::var("AWS_DEFAULT_REGION").unwrap_or_else(|_| "us-east-1".to_string());

        Ok(Self {
            bucket: std::env::var("AWS_DEFAULT_BUCKET").unwrap_or_default(),
            region,
            access_key_id: std::env::var("AWS_ACCESS_KEY_ID").ok(),
            secret_access_key: std::env::var("AWS_SECRET_ACCESS_KEY").ok(),
            endpoint: std::env::var("S3_ENDPOINT").ok(),
        })
    }
}

// ---------------------------------------------------------------------------
// S3StoreProvider
// ---------------------------------------------------------------------------

/// Object store provider backed by Amazon S3 (or an S3-compatible service such
/// as MinIO, Ceph, or Cloudflare R2).
pub struct S3StoreProvider {
    store: Arc<dyn ObjectStore>,
    region: String,
}

impl S3StoreProvider {
    /// Build an [`S3StoreProvider`] from the supplied [`S3Config`].
    pub fn new(cfg: S3Config) -> Result<Self> {
        let mut builder = AmazonS3Builder::new()
            .with_region(&cfg.region)
            .with_bucket_name(&cfg.bucket);

        if let Some(key) = cfg.access_key_id {
            builder = builder.with_access_key_id(&key);
        }
        if let Some(secret) = cfg.secret_access_key {
            builder = builder.with_secret_access_key(&secret);
        }
        if let Some(endpoint) = cfg.endpoint {
            builder = builder.with_endpoint(&endpoint);
        }

        let store = builder
            .build()
            .map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self {
            store: Arc::new(store),
            region: cfg.region,
        })
    }

    /// Expose the underlying region string.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Convert a string key to an [`object_store`] [`Path`][StoragePath].
    fn to_path(key: &str) -> StoragePath {
        StoragePath::from(key)
    }

    /// Map an [`object_store::Error`] to our [`StorageError`] with context.
    fn map_store_error(err: object_store::Error, bucket: &str, key: &str) -> StorageError {
        match err {
            object_store::Error::NotFound { .. } => StorageError::NotFound {
                bucket: bucket.to_string(),
                key: key.to_string(),
            },
            other => StorageError::Provider(other.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// ObjectStoreProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl ObjectStoreProvider for S3StoreProvider {
    fn provider_name(&self) -> &str {
        "s3"
    }

    #[instrument(skip_all, fields(bucket, key))]
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        _meta: ObjectMeta,
    ) -> Result<PutResult> {
        debug!(bucket, key, bytes = data.len(), "S3 put_object");
        let path = Self::to_path(key);
        self.store
            .put(&path, data.into())
            .await
            .map_err(|e| Self::map_store_error(e, bucket, key))?;

        Ok(PutResult {
            key: key.to_string(),
            etag: None,
            version_id: None,
        })
    }

    #[instrument(skip(self), fields(bucket, key))]
    async fn get_object(&self, bucket: &str, key: &str) -> Result<ObjectData> {
        debug!(bucket, key, "S3 get_object");
        let path = Self::to_path(key);
        let result = self
            .store
            .get(&path)
            .await
            .map_err(|e| Self::map_store_error(e, bucket, key))?;

        let data = result
            .bytes()
            .await
            .map_err(|e| Self::map_store_error(e, bucket, key))?;

        Ok(ObjectData {
            key: key.to_string(),
            data,
            meta: ObjectMeta::default(),
        })
    }

    #[instrument(skip(self), fields(bucket, key))]
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        debug!(bucket, key, "S3 delete_object");
        let path = Self::to_path(key);
        self.store
            .delete(&path)
            .await
            .map_err(|e| Self::map_store_error(e, bucket, key))?;
        Ok(())
    }

    #[instrument(skip(self), fields(bucket, prefix, max))]
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        max: usize,
    ) -> Result<Vec<ObjectListing>> {
        use futures::StreamExt as _;

        debug!(bucket, prefix, max, "S3 list_objects");

        let prefix_path = prefix.map(StoragePath::from);
        let mut stream = self.store.list(prefix_path.as_ref());

        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let meta = item.map_err(|e| StorageError::Provider(e.to_string()))?;
            results.push(ObjectListing {
                key: meta.location.to_string(),
                size: meta.size as u64,
                last_modified: Some(meta.last_modified),
                etag: meta.e_tag,
            });
            if results.len() >= max {
                break;
            }
        }

        Ok(results)
    }

    async fn presigned_url(
        &self,
        _bucket: &str,
        _key: &str,
        _ttl: Duration,
        _op: PresignOp,
    ) -> Result<String> {
        // The `object_store` 0.11 public API does not expose presigned-URL
        // generation on the `ObjectStore` trait.  Presigned URLs are available
        // through provider-specific extensions that are not yet stabilised.
        Err(StorageError::Unsupported(
            "S3 presigned URLs require signed URL support not yet available in the public \
             object_store trait surface; implement via the aws-sdk-s3 crate directly if needed"
                .to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_config_from_env_defaults() {
        // Ensure no interfering env vars are set.
        std::env::remove_var("AWS_DEFAULT_REGION");
        std::env::remove_var("AWS_DEFAULT_BUCKET");
        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
        std::env::remove_var("S3_ENDPOINT");

        let cfg = S3Config::from_env().expect("from_env should succeed with no vars set");
        assert_eq!(cfg.region, "us-east-1", "default region must be us-east-1");
        assert_eq!(cfg.bucket, "");
        assert!(cfg.access_key_id.is_none());
        assert!(cfg.secret_access_key.is_none());
        assert!(cfg.endpoint.is_none());
    }

    #[test]
    fn test_s3_config_with_endpoint() {
        std::env::set_var("S3_ENDPOINT", "http://localhost:9000");
        std::env::set_var("AWS_DEFAULT_REGION", "eu-west-1");

        let cfg = S3Config::from_env().expect("from_env should succeed");
        assert_eq!(
            cfg.endpoint.as_deref(),
            Some("http://localhost:9000"),
            "endpoint must be captured from S3_ENDPOINT"
        );
        assert_eq!(cfg.region, "eu-west-1");

        // Cleanup so we don't leak env state into other tests.
        std::env::remove_var("S3_ENDPOINT");
        std::env::remove_var("AWS_DEFAULT_REGION");
    }

    #[test]
    fn test_s3_config_with_credentials() {
        std::env::set_var("AWS_ACCESS_KEY_ID", "AKIATEST");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "secret123");

        let cfg = S3Config::from_env().expect("from_env should succeed");
        assert_eq!(cfg.access_key_id.as_deref(), Some("AKIATEST"));
        assert_eq!(cfg.secret_access_key.as_deref(), Some("secret123"));

        std::env::remove_var("AWS_ACCESS_KEY_ID");
        std::env::remove_var("AWS_SECRET_ACCESS_KEY");
    }
}
