//! AWS S3 (and S3-compatible) object store provider.
//!
//! Enabled via the `aws` Cargo feature.

use std::time::Duration;

use bytes::Bytes;
use oxistore_blob::{BlobError, BlobStore};
use oxistore_blob_s3::{S3BlobStore, S3BlobStoreBuilder, S3Credentials};
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
    store: S3BlobStore,
    region: String,
}

impl S3StoreProvider {
    /// Build an [`S3StoreProvider`] from the supplied [`S3Config`].
    ///
    /// # Credentials
    ///
    /// When both `access_key_id` and `secret_access_key` are present in the
    /// supplied [`S3Config`], they are used directly.  Otherwise credentials
    /// are read from the standard AWS environment variables
    /// (`AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY`) via
    /// [`S3Credentials::from_env`].
    ///
    /// **Capability change:** unlike the previous `object_store`-based
    /// implementation, the pure-Rust `oxistore-blob-s3` backend does **not**
    /// resolve ambient IAM instance-profile / container credentials.  Only
    /// explicit credentials or the AWS environment variables are supported;
    /// this is an intentional, documented change.
    pub fn new(cfg: S3Config) -> Result<Self> {
        let region = cfg.region.clone();

        let credentials = if let (Some(access_key), Some(secret_key)) =
            (cfg.access_key_id, cfg.secret_access_key)
        {
            S3Credentials::new(access_key, secret_key, None)
        } else {
            S3Credentials::from_env().map_err(|e| StorageError::Config(e.to_string()))?
        };

        // Synthesize a standard regional AWS endpoint when the caller did not
        // supply a custom one.  A custom endpoint (MinIO, Ceph, R2, …) implies
        // path-style addressing; the synthesized AWS default uses virtual-host
        // style, so `path_style` is only enabled for an explicit endpoint.
        let (endpoint, path_style) = match cfg.endpoint {
            Some(custom) => (custom, true),
            None => (format!("https://s3.{region}.amazonaws.com"), false),
        };

        let store = S3BlobStoreBuilder::new()
            .endpoint(endpoint)
            .region(&region)
            .bucket(&cfg.bucket)
            .credentials(credentials)
            .path_style(path_style)
            .build()
            .map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self { store, region })
    }

    /// Expose the underlying region string.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Map a [`BlobError`] to our [`StorageError`] with bucket/key context.
    fn map_store_error(err: BlobError, bucket: &str, key: &str) -> StorageError {
        match err {
            BlobError::NotFound(_) => StorageError::NotFound {
                bucket: bucket.to_string(),
                key: key.to_string(),
            },
            // `BlobError` is `#[non_exhaustive]`, so a wildcard arm is mandatory.
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
        self.store
            .put(key, data)
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
        let data = self
            .store
            .get(key)
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
        self.store
            .delete(key)
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
        debug!(bucket, prefix, max, "S3 list_objects");

        // `oxistore-blob-s3` exposes only an eager, key-only `list`; there is no
        // metadata-bearing or bounded/paginated variant.  Fetch the full key
        // list, then issue a bounded number of `head` calls (capped at `max`)
        // to gather sizes.  Keys removed between `list` and `head` (a natural
        // race) are silently skipped.
        let prefix_str = prefix.unwrap_or("");
        let keys = self
            .store
            .list(prefix_str)
            .await
            .map_err(|e| Self::map_store_error(e, bucket, prefix_str))?;

        let mut results = Vec::new();
        for key in keys.into_iter().take(max) {
            match self.store.head(&key).await {
                Ok(meta) => results.push(ObjectListing {
                    key: meta.key,
                    size: meta.size,
                    last_modified: None,
                    etag: None,
                }),
                Err(BlobError::NotFound(_)) => continue,
                Err(e) => return Err(Self::map_store_error(e, bucket, &key)),
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
        // The underlying `S3BlobStore` provides real, offline SigV4
        // `presign_get` / `presign_put` inherent methods.  Wiring them through
        // this trait method is a deliberate future enhancement, out of scope
        // for the object_store -> oxistore-blob migration.
        Err(StorageError::Unsupported(
            "S3 presigned URLs are not yet wired through the ObjectStoreProvider trait; \
             S3BlobStore::presign_get / presign_put exist and could back this in a future \
             enhancement"
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
