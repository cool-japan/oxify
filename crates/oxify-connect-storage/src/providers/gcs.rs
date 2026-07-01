//! Google Cloud Storage object store provider.
//!
//! Enabled via the `gcs` Cargo feature.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use object_store::{gcp::GoogleCloudStorageBuilder, path::Path as StoragePath, ObjectStore, ObjectStoreExt};
use tracing::{debug, instrument};

use super::ObjectStoreProvider;
use crate::{
    errors::{Result, StorageError},
    types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult},
};

// ---------------------------------------------------------------------------
// GcsConfig
// ---------------------------------------------------------------------------

/// Configuration for the Google Cloud Storage provider.
#[derive(Debug, Clone)]
pub struct GcsConfig {
    /// Default bucket name used when the caller does not override it.
    pub bucket: String,
    /// Path to a service account JSON key file on the local filesystem.
    /// Falls back to Application Default Credentials (ADC) when `None`.
    pub service_account_key_path: Option<String>,
    /// A service account JSON key provided as a raw string rather than a
    /// file path.  Mutually exclusive with `service_account_key_path`; the
    /// path variant takes precedence if both are supplied.
    pub service_account_key_json: Option<String>,
}

impl GcsConfig {
    /// Populate a [`GcsConfig`] from well-known environment variables.
    ///
    /// | Variable | Field |
    /// |---|---|
    /// | `GCS_BUCKET` | `bucket` (defaults to `""`) |
    /// | `GOOGLE_APPLICATION_CREDENTIALS` | `service_account_key_path` |
    /// | `GCS_SERVICE_ACCOUNT_KEY` | `service_account_key_json` |
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            bucket: std::env::var("GCS_BUCKET").unwrap_or_default(),
            service_account_key_path: std::env::var("GOOGLE_APPLICATION_CREDENTIALS").ok(),
            service_account_key_json: std::env::var("GCS_SERVICE_ACCOUNT_KEY").ok(),
        })
    }
}

// ---------------------------------------------------------------------------
// GcsStoreProvider
// ---------------------------------------------------------------------------

/// Object store provider backed by Google Cloud Storage.
pub struct GcsStoreProvider {
    store: Arc<dyn ObjectStore>,
}

impl GcsStoreProvider {
    /// Build a [`GcsStoreProvider`] from the supplied [`GcsConfig`].
    pub fn new(cfg: GcsConfig) -> Result<Self> {
        let mut builder = GoogleCloudStorageBuilder::new().with_bucket_name(&cfg.bucket);

        if let Some(path) = &cfg.service_account_key_path {
            builder = builder.with_service_account_path(path);
        }
        if let Some(json) = &cfg.service_account_key_json {
            builder = builder.with_service_account_key(json);
        }

        let store = builder
            .build()
            .map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// Build a [`GcsStoreProvider`] by reading configuration from the
    /// environment.  See [`GcsConfig::from_env`] for the variable mapping.
    pub fn from_env() -> Result<Self> {
        Self::new(GcsConfig::from_env()?)
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
impl ObjectStoreProvider for GcsStoreProvider {
    fn provider_name(&self) -> &str {
        "gcs"
    }

    #[instrument(skip_all, fields(bucket, key))]
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        _meta: ObjectMeta,
    ) -> Result<PutResult> {
        debug!(bucket, key, bytes = data.len(), "GCS put_object");
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
        debug!(bucket, key, "GCS get_object");
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
        debug!(bucket, key, "GCS delete_object");
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

        debug!(bucket, prefix, max, "GCS list_objects");

        let prefix_path = prefix.map(StoragePath::from);
        let mut stream = self.store.list(prefix_path.as_ref());

        let mut results = Vec::new();
        while let Some(item) = stream.next().await {
            let meta = item.map_err(|e| StorageError::Provider(e.to_string()))?;
            results.push(ObjectListing {
                key: meta.location.to_string(),
                size: meta.size,
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
        Err(StorageError::Unsupported(
            "GCS presigned URLs require signed URL support not yet available in the public \
             object_store trait surface"
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
    fn test_gcs_config_from_env_defaults() {
        std::env::remove_var("GCS_BUCKET");
        std::env::remove_var("GOOGLE_APPLICATION_CREDENTIALS");
        std::env::remove_var("GCS_SERVICE_ACCOUNT_KEY");

        let cfg = GcsConfig::from_env().expect("from_env ok");
        assert_eq!(cfg.bucket, "");
        assert!(cfg.service_account_key_path.is_none());
        assert!(cfg.service_account_key_json.is_none());
    }

    #[test]
    fn test_gcs_config_with_credentials_path() {
        std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", "/tmp/key.json");
        std::env::set_var("GCS_BUCKET", "my-bucket");

        let cfg = GcsConfig::from_env().expect("from_env ok");
        assert_eq!(
            cfg.service_account_key_path.as_deref(),
            Some("/tmp/key.json")
        );
        assert_eq!(cfg.bucket, "my-bucket");

        std::env::remove_var("GOOGLE_APPLICATION_CREDENTIALS");
        std::env::remove_var("GCS_BUCKET");
    }

    #[test]
    fn test_gcs_config_with_key_json() {
        std::env::set_var("GCS_SERVICE_ACCOUNT_KEY", r#"{"type":"service_account"}"#);

        let cfg = GcsConfig::from_env().expect("from_env ok");
        assert_eq!(
            cfg.service_account_key_json.as_deref(),
            Some(r#"{"type":"service_account"}"#)
        );

        std::env::remove_var("GCS_SERVICE_ACCOUNT_KEY");
    }
}
