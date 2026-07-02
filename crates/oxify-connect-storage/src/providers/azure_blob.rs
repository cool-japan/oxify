//! Azure Blob Storage object store provider.
//!
//! Enabled via the `azure` Cargo feature.

use std::time::Duration;

use bytes::Bytes;
use oxistore_blob::{BlobError, BlobStore};
use oxistore_blob_azure::{AzureBlobStore, AzureConfig, AzureCredentials};
use tracing::{debug, instrument};

use super::ObjectStoreProvider;
use crate::{
    errors::{Result, StorageError},
    types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult},
};

// ---------------------------------------------------------------------------
// AzureBlobConfig
// ---------------------------------------------------------------------------

/// Configuration for the Azure Blob Storage provider.
#[derive(Debug, Clone)]
pub struct AzureBlobConfig {
    /// Name of the blob container (analogous to an S3 bucket).
    pub container: String,
    /// Azure storage account name.
    pub account_name: String,
    /// Storage account access key.  Falls back to managed-identity /
    /// workload-identity credentials when `None`.
    pub account_key: Option<String>,
    /// Custom endpoint URL, useful for Azurite (local emulator) or
    /// sovereign-cloud deployments.
    pub endpoint: Option<String>,
}

impl AzureBlobConfig {
    /// Populate an [`AzureBlobConfig`] from well-known environment variables.
    ///
    /// | Variable | Field |
    /// |---|---|
    /// | `AZURE_STORAGE_CONTAINER` | `container` (defaults to `""`) |
    /// | `AZURE_STORAGE_ACCOUNT` | `account_name` (defaults to `""`) |
    /// | `AZURE_STORAGE_KEY` | `account_key` |
    /// | `AZURE_STORAGE_ENDPOINT` | `endpoint` |
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            container: std::env::var("AZURE_STORAGE_CONTAINER").unwrap_or_default(),
            account_name: std::env::var("AZURE_STORAGE_ACCOUNT").unwrap_or_default(),
            account_key: std::env::var("AZURE_STORAGE_KEY").ok(),
            endpoint: std::env::var("AZURE_STORAGE_ENDPOINT").ok(),
        })
    }
}

// ---------------------------------------------------------------------------
// AzureBlobStoreProvider
// ---------------------------------------------------------------------------

/// Object store provider backed by Azure Blob Storage.
pub struct AzureBlobStoreProvider {
    store: AzureBlobStore,
}

impl AzureBlobStoreProvider {
    /// Build an [`AzureBlobStoreProvider`] from the supplied
    /// [`AzureBlobConfig`].
    ///
    /// # Credentials
    ///
    /// The pure-Rust `oxistore-blob-azure` backend authenticates exclusively
    /// with a static Shared Key (account name + account key).  It does **not**
    /// support managed identity / workload identity.  When
    /// [`AzureBlobConfig::account_key`] is `None`, this returns a
    /// [`StorageError::Config`] up front rather than silently failing later at
    /// request time.
    pub fn new(cfg: AzureBlobConfig) -> Result<Self> {
        let account_key = cfg.account_key.ok_or_else(|| {
            StorageError::Config(
                "the pure-Rust Azure Blob backend requires an explicit account key; \
                 managed identity / workload identity is not supported"
                    .to_string(),
            )
        })?;

        let credentials = AzureCredentials {
            account_name: cfg.account_name,
            account_key_b64: account_key,
        };

        let mut config = AzureConfig::new(credentials, cfg.container);
        if let Some(endpoint) = cfg.endpoint {
            config.endpoint = Some(endpoint);
        }

        let store = AzureBlobStore::new(config).map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self { store })
    }

    /// Build an [`AzureBlobStoreProvider`] by reading configuration from the
    /// environment.  See [`AzureBlobConfig::from_env`] for the variable
    /// mapping.
    pub fn from_env() -> Result<Self> {
        Self::new(AzureBlobConfig::from_env()?)
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
impl ObjectStoreProvider for AzureBlobStoreProvider {
    fn provider_name(&self) -> &str {
        "azure-blob"
    }

    #[instrument(skip_all, fields(bucket, key))]
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        _meta: ObjectMeta,
    ) -> Result<PutResult> {
        debug!(bucket, key, bytes = data.len(), "Azure put_object");
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
        debug!(bucket, key, "Azure get_object");
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
        debug!(bucket, key, "Azure delete_object");
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
        debug!(bucket, prefix, max, "Azure list_objects");

        // `oxistore-blob-azure` exposes only an eager, key-only `list`; there is
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
        // `oxistore-blob-azure` provides no SAS-token / signed-URL capability at
        // all, so this operation is genuinely unsupported by the backend.
        Err(StorageError::Unsupported(
            "Azure Blob presigned URLs are not supported: the pure-Rust oxistore-blob-azure \
             backend exposes no SAS-token generation"
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
    fn test_azure_blob_config_from_env_defaults() {
        std::env::remove_var("AZURE_STORAGE_CONTAINER");
        std::env::remove_var("AZURE_STORAGE_ACCOUNT");
        std::env::remove_var("AZURE_STORAGE_KEY");
        std::env::remove_var("AZURE_STORAGE_ENDPOINT");

        let cfg = AzureBlobConfig::from_env().expect("from_env ok");
        assert_eq!(cfg.container, "");
        assert_eq!(cfg.account_name, "");
        assert!(cfg.account_key.is_none());
        assert!(cfg.endpoint.is_none());
    }

    #[test]
    fn test_azure_blob_config_with_account_key() {
        std::env::set_var("AZURE_STORAGE_ACCOUNT", "mystorageaccount");
        std::env::set_var("AZURE_STORAGE_CONTAINER", "my-container");
        std::env::set_var("AZURE_STORAGE_KEY", "base64encodedkey==");

        let cfg = AzureBlobConfig::from_env().expect("from_env ok");
        assert_eq!(cfg.account_name, "mystorageaccount");
        assert_eq!(cfg.container, "my-container");
        assert_eq!(cfg.account_key.as_deref(), Some("base64encodedkey=="));

        std::env::remove_var("AZURE_STORAGE_ACCOUNT");
        std::env::remove_var("AZURE_STORAGE_CONTAINER");
        std::env::remove_var("AZURE_STORAGE_KEY");
    }

    #[test]
    fn test_azure_blob_config_with_endpoint() {
        std::env::set_var("AZURE_STORAGE_ENDPOINT", "http://localhost:10000");

        let cfg = AzureBlobConfig::from_env().expect("from_env ok");
        assert_eq!(cfg.endpoint.as_deref(), Some("http://localhost:10000"));

        std::env::remove_var("AZURE_STORAGE_ENDPOINT");
    }
}
