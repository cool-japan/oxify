//! Google Cloud Storage object store provider.
//!
//! Enabled via the `gcs` Cargo feature.

use std::time::Duration;

use bytes::Bytes;
use oxistore_blob::{BlobError, BlobStore};
use oxistore_blob_gcs::{GcsBlobStore, GcsConfig as GcsBlobConfig, GcsServiceAccount};
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
    store: GcsBlobStore,
}

impl GcsStoreProvider {
    /// Build a [`GcsStoreProvider`] from the supplied [`GcsConfig`].
    ///
    /// Credentials are resolved in the same precedence order as the previous
    /// implementation: a service-account **file path** takes priority, then an
    /// inline JSON key, and finally Application Default Credentials sourced
    /// from `GOOGLE_APPLICATION_CREDENTIALS` via [`GcsServiceAccount::from_env`].
    pub fn new(cfg: GcsConfig) -> Result<Self> {
        let credentials = if let Some(path) = &cfg.service_account_key_path {
            GcsServiceAccount::from_json_file(path)
                .map_err(|e| StorageError::Config(e.to_string()))?
        } else if let Some(json) = &cfg.service_account_key_json {
            GcsServiceAccount::from_json_str(json)
                .map_err(|e| StorageError::Config(e.to_string()))?
        } else {
            GcsServiceAccount::from_env().map_err(|e| StorageError::Config(e.to_string()))?
        };

        let config = GcsBlobConfig {
            bucket: cfg.bucket,
            credentials,
            timeout: Duration::from_secs(30),
            endpoint: None,
            oauth_endpoint: None,
        };

        let store = GcsBlobStore::new(config).map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self { store })
    }

    /// Build a [`GcsStoreProvider`] by reading configuration from the
    /// environment.  See [`GcsConfig::from_env`] for the variable mapping.
    pub fn from_env() -> Result<Self> {
        Self::new(GcsConfig::from_env()?)
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
        debug!(bucket, key, "GCS get_object");
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
        debug!(bucket, key, "GCS delete_object");
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
        debug!(bucket, prefix, max, "GCS list_objects");

        // `oxistore-blob-gcs` exposes only an eager, key-only `list`; there is
        // no metadata-bearing or bounded/paginated variant.  Fetch the full key
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
        // `oxistore-blob-gcs` has no presigned-URL / signed-URL capability at
        // all, so this operation is genuinely unsupported by the backend.
        Err(StorageError::Unsupported(
            "GCS presigned URLs are not supported: the pure-Rust oxistore-blob-gcs backend \
             exposes no signed-URL generation"
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
