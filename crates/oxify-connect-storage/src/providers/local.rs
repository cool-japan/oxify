//! Local filesystem object store provider.
//!
//! Enabled via the `local` Cargo feature.  All operations are sandboxed to a
//! configurable root directory; the root is passed to
//! [`object_store::local::LocalFileSystem::new_with_prefix`] which prevents
//! path traversal outside the sandbox.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::StreamExt as _;
use object_store::{
    local::LocalFileSystem, path::Path as StoragePath, ObjectStore, ObjectStoreExt,
};
use tracing::{debug, instrument};

use super::ObjectStoreProvider;
use crate::{
    errors::{Result, StorageError},
    types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult},
};

// ---------------------------------------------------------------------------
// LocalFsConfig
// ---------------------------------------------------------------------------

/// Configuration for the local filesystem store.
#[derive(Debug, Clone)]
pub struct LocalFsConfig {
    /// Root directory used as the sandbox for all object store operations.
    ///
    /// Every path passed to the provider is resolved relative to this root,
    /// so callers cannot escape the directory tree via `..` sequences.
    pub root_path: PathBuf,

    /// When `true`, `root_path` is created (including all parents) if it does
    /// not already exist.  Defaults to `true`.
    pub create_if_missing: bool,
}

impl Default for LocalFsConfig {
    fn default() -> Self {
        Self {
            root_path: std::env::temp_dir().join("oxify-local-store"),
            create_if_missing: true,
        }
    }
}

impl LocalFsConfig {
    /// Populate a [`LocalFsConfig`] from environment variables.
    ///
    /// | Variable | Field |
    /// |---|---|
    /// | `LOCAL_STORE_ROOT` | `root_path` (required) |
    ///
    /// Returns [`StorageError::Config`] when `LOCAL_STORE_ROOT` is not set.
    pub fn from_env() -> Result<Self> {
        let root_str = std::env::var("LOCAL_STORE_ROOT").map_err(|_| {
            StorageError::Config(
                "LOCAL_STORE_ROOT environment variable is required for local filesystem provider"
                    .to_string(),
            )
        })?;

        Ok(Self {
            root_path: PathBuf::from(root_str),
            create_if_missing: true,
        })
    }
}

// ---------------------------------------------------------------------------
// LocalFsProvider
// ---------------------------------------------------------------------------

/// Object store provider backed by the local filesystem.
///
/// All paths are sandboxed to [`LocalFsConfig::root_path`] via
/// [`LocalFileSystem::new_with_prefix`].  The `bucket` argument in each
/// operation is treated as a sub-directory under the root, and `key` as the
/// relative path within that sub-directory, so the on-disk layout is
/// `<root>/<bucket>/<key>`.
pub struct LocalFsProvider {
    /// The underlying [`object_store`] store, sandboxed to `root_path`.
    store: Arc<dyn ObjectStore>,
    /// The root directory of this provider instance.
    root_path: PathBuf,
}

impl LocalFsProvider {
    /// Build a [`LocalFsProvider`] from the supplied [`LocalFsConfig`].
    ///
    /// If `cfg.create_if_missing` is `true` and `cfg.root_path` does not
    /// exist, the directory tree is created with
    /// [`std::fs::create_dir_all`].
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Config`] when:
    /// - The root directory does not exist and `create_if_missing` is `false`.
    /// - The directory cannot be created.
    /// - The path cannot be canonicalized by [`LocalFileSystem::new_with_prefix`].
    pub fn new(cfg: LocalFsConfig) -> Result<Self> {
        if cfg.create_if_missing {
            std::fs::create_dir_all(&cfg.root_path)
                .map_err(|e| StorageError::Config(format!("cannot create root dir: {e}")))?;
        }

        let store = LocalFileSystem::new_with_prefix(&cfg.root_path)
            .map_err(|e| StorageError::Config(e.to_string()))?;

        Ok(Self {
            store: Arc::new(store),
            root_path: cfg.root_path,
        })
    }

    /// The root sandbox directory this provider was initialised with.
    pub fn root_path(&self) -> &std::path::Path {
        &self.root_path
    }

    /// Build the [`StoragePath`] for a `(bucket, key)` pair.
    ///
    /// The resulting path is `{bucket}/{key}` which, when interpreted by the
    /// sandboxed [`LocalFileSystem`], maps to `<root>/<bucket>/<key>` on disk.
    #[inline]
    fn to_path(bucket: &str, key: &str) -> StoragePath {
        StoragePath::from(format!("{bucket}/{key}"))
    }

    /// Map an [`object_store::Error`] to our [`StorageError`] with bucket/key
    /// context attached.
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
impl ObjectStoreProvider for LocalFsProvider {
    fn provider_name(&self) -> &str {
        "local"
    }

    /// Write `data` to `<root>/<bucket>/<key>`.
    ///
    /// Parent directories are created automatically by the underlying
    /// [`LocalFileSystem`].
    #[instrument(skip_all, fields(bucket, key))]
    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        _meta: ObjectMeta,
    ) -> Result<PutResult> {
        debug!(bucket, key, bytes = data.len(), "LocalFs put_object");
        let path = Self::to_path(bucket, key);
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

    /// Read the full contents of `<root>/<bucket>/<key>`.
    ///
    /// Returns [`StorageError::NotFound`] when the file does not exist.
    #[instrument(skip(self), fields(bucket, key))]
    async fn get_object(&self, bucket: &str, key: &str) -> Result<ObjectData> {
        debug!(bucket, key, "LocalFs get_object");
        let path = Self::to_path(bucket, key);
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

    /// Delete `<root>/<bucket>/<key>`.
    ///
    /// Returns [`StorageError::NotFound`] when the file does not exist.
    #[instrument(skip(self), fields(bucket, key))]
    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        debug!(bucket, key, "LocalFs delete_object");
        let path = Self::to_path(bucket, key);
        self.store
            .delete(&path)
            .await
            .map_err(|e| Self::map_store_error(e, bucket, key))
    }

    /// List at most `max` objects whose paths begin with `<bucket>/<prefix>`.
    ///
    /// The keys in the returned [`ObjectListing`] entries include the bucket
    /// segment, so they look like `bucket/key/path`.  Pass `None` for `prefix`
    /// to enumerate everything under the bucket sub-directory.
    #[instrument(skip(self), fields(bucket, prefix, max))]
    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        max: usize,
    ) -> Result<Vec<ObjectListing>> {
        debug!(bucket, prefix, max, "LocalFs list_objects");

        let prefix_str = match prefix {
            Some(p) => format!("{bucket}/{p}"),
            None => format!("{bucket}/"),
        };
        let prefix_path = StoragePath::from(prefix_str);

        let mut stream = self.store.list(Some(&prefix_path));
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

    /// Not supported — local files cannot produce presigned URLs.
    ///
    /// Always returns [`StorageError::Unsupported`].
    async fn presigned_url(
        &self,
        _bucket: &str,
        _key: &str,
        _ttl: Duration,
        _op: PresignOp,
    ) -> Result<String> {
        Err(StorageError::Unsupported(
            "Local filesystem does not support presigned URLs".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a [`LocalFsProvider`] sandboxed to a fresh temporary directory.
    fn make_provider(dir: &tempfile::TempDir) -> LocalFsProvider {
        let cfg = LocalFsConfig {
            root_path: dir.path().to_path_buf(),
            create_if_missing: false, // TempDir already exists
        };
        LocalFsProvider::new(cfg).expect("provider creation must succeed")
    }

    /// Helper: build test bytes from a static string slice.
    fn make_bytes(s: &str) -> Bytes {
        Bytes::from(s.to_owned())
    }

    #[test]
    fn provider_name_is_local() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);
        assert_eq!(provider.provider_name(), "local");
    }

    #[tokio::test]
    async fn put_and_get_object() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);
        let payload = make_bytes("hello local store");

        provider
            .put_object(
                "bucket",
                "greet.txt",
                payload.clone(),
                ObjectMeta::default(),
            )
            .await
            .expect("put_object must succeed");

        let retrieved = provider
            .get_object("bucket", "greet.txt")
            .await
            .expect("get_object must succeed");

        assert_eq!(retrieved.data, payload, "round-tripped bytes must match");
        assert_eq!(retrieved.key, "greet.txt");
    }

    #[tokio::test]
    async fn get_nonexistent_returns_not_found() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);

        let err = provider
            .get_object("bucket", "ghost.txt")
            .await
            .expect_err("get on missing key must return error");

        assert!(
            matches!(err, StorageError::NotFound { .. }),
            "expected NotFound, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn delete_object_succeeds() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);

        provider
            .put_object(
                "bucket",
                "to-delete.bin",
                make_bytes("data"),
                ObjectMeta::default(),
            )
            .await
            .expect("put must succeed");

        provider
            .delete_object("bucket", "to-delete.bin")
            .await
            .expect("delete must succeed");

        let err = provider
            .get_object("bucket", "to-delete.bin")
            .await
            .expect_err("get after delete must fail");

        assert!(
            matches!(err, StorageError::NotFound { .. }),
            "expected NotFound after delete, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn list_objects_empty_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);

        for i in 0..3_u8 {
            provider
                .put_object(
                    "mybucket",
                    &format!("obj-{i}.bin"),
                    make_bytes("x"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put must succeed");
        }

        let listed = provider
            .list_objects("mybucket", None, 100)
            .await
            .expect("list must succeed");

        assert_eq!(listed.len(), 3, "all 3 objects must be listed");
    }

    #[tokio::test]
    async fn list_objects_with_prefix() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);

        // 3 objects under "logs/" prefix, 2 under "metrics/"
        for i in 0..3_u8 {
            provider
                .put_object(
                    "b",
                    &format!("logs/entry-{i}.log"),
                    make_bytes("log data"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put must succeed");
        }
        for i in 0..2_u8 {
            provider
                .put_object(
                    "b",
                    &format!("metrics/m-{i}.dat"),
                    make_bytes("metric data"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put must succeed");
        }

        let listed = provider
            .list_objects("b", Some("logs/"), 100)
            .await
            .expect("list must succeed");

        assert_eq!(listed.len(), 3, "only the 3 'logs/' objects must be listed");
        assert!(
            listed.iter().all(|o| o.key.contains("logs/")),
            "all results must be under the logs/ prefix"
        );
    }

    #[tokio::test]
    async fn presigned_url_unsupported() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);

        let err = provider
            .presigned_url("bucket", "key", Duration::from_secs(300), PresignOp::Get)
            .await
            .expect_err("presigned_url must return an error for local provider");

        assert!(
            matches!(err, StorageError::Unsupported(_)),
            "expected Unsupported, got: {err:?}"
        );
    }

    #[test]
    fn create_if_missing_creates_directory() {
        let base = std::env::temp_dir().join(format!(
            "oxify-local-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));

        // Ensure the target does not already exist.
        let _ = std::fs::remove_dir_all(&base);

        let cfg = LocalFsConfig {
            root_path: base.clone(),
            create_if_missing: true,
        };

        let provider = LocalFsProvider::new(cfg).expect("must succeed when create_if_missing=true");
        assert!(
            base.exists(),
            "root directory must have been created on disk"
        );
        assert_eq!(provider.root_path(), base.canonicalize().unwrap());

        // Cleanup.
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn from_env_missing_var() {
        std::env::remove_var("LOCAL_STORE_ROOT");

        let err = LocalFsConfig::from_env()
            .expect_err("from_env must fail when LOCAL_STORE_ROOT is unset");

        assert!(
            matches!(err, StorageError::Config(_)),
            "expected Config error, got: {err:?}"
        );
    }

    #[test]
    fn default_config_uses_temp_dir() {
        let cfg = LocalFsConfig::default();
        assert!(
            cfg.root_path.starts_with(std::env::temp_dir()),
            "default root_path must be inside the system temp directory"
        );
    }

    #[tokio::test]
    async fn list_objects_max_cap() {
        let dir = tempfile::TempDir::new().unwrap();
        let provider = make_provider(&dir);

        for i in 0..5_u8 {
            provider
                .put_object(
                    "cap-bucket",
                    &format!("item-{i:02}.bin"),
                    make_bytes("data"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put must succeed");
        }

        let listed = provider
            .list_objects("cap-bucket", None, 3)
            .await
            .expect("list must succeed");

        assert_eq!(listed.len(), 3, "max=3 must cap results at 3");
    }
}
