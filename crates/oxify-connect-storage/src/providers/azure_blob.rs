//! Azure Blob Storage object store provider.
//!
//! Enabled via the `azure` Cargo feature.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use object_store::{azure::MicrosoftAzureBuilder, path::Path as StoragePath, ObjectStore};
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
    store: Arc<dyn ObjectStore>,
}

impl AzureBlobStoreProvider {
    /// Build an [`AzureBlobStoreProvider`] from the supplied
    /// [`AzureBlobConfig`].
    pub fn new(cfg: AzureBlobConfig) -> Result<Self> {
        let mut builder = MicrosoftAzureBuilder::new()
            .with_container_name(&cfg.container)
            .with_account(&cfg.account_name);

        if let Some(key) = &cfg.account_key {
            builder = builder.with_access_key(key);
        }
        if let Some(endpoint) = cfg.endpoint {
            builder = builder.with_endpoint(endpoint);
        }

        let store = builder
            .build()
            .map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// Build an [`AzureBlobStoreProvider`] by reading configuration from the
    /// environment.  See [`AzureBlobConfig::from_env`] for the variable
    /// mapping.
    pub fn from_env() -> Result<Self> {
        Self::new(AzureBlobConfig::from_env()?)
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
        debug!(bucket, key, "Azure get_object");
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
        debug!(bucket, key, "Azure delete_object");
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

        debug!(bucket, prefix, max, "Azure list_objects");

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
        Err(StorageError::Unsupported(
            "Azure Blob presigned URLs require SAS token generation not yet available in the \
             public object_store trait surface"
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
