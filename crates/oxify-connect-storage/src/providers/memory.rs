use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::sync::RwLock;

use super::ObjectStoreProvider;
use crate::{
    errors::{Result, StorageError},
    types::{ObjectData, ObjectListing, ObjectMeta, PresignOp, PutResult},
};

// ---------------------------------------------------------------------------
// Internal type alias
// ---------------------------------------------------------------------------

/// The inner map type: `(bucket, key) -> (bytes, metadata)`.
type StoreMap = HashMap<(String, String), (Bytes, ObjectMeta)>;

/// A shared, async-aware reference to the in-memory store map.
type SharedStore = Arc<RwLock<StoreMap>>;

// ---------------------------------------------------------------------------
// MemoryStoreProvider
// ---------------------------------------------------------------------------

/// An in-memory object store for development and testing.
///
/// Objects are indexed by `(bucket, key)` pairs and stored as raw [`Bytes`]
/// together with their [`ObjectMeta`].  All operations are thread-safe via a
/// [`tokio::sync::RwLock`].
pub struct MemoryStoreProvider {
    store: SharedStore,
}

impl MemoryStoreProvider {
    /// Create a new, empty in-memory store.
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

impl Default for MemoryStoreProvider {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ObjectStoreProvider implementation
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl ObjectStoreProvider for MemoryStoreProvider {
    fn provider_name(&self) -> &str {
        "memory"
    }

    async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        data: Bytes,
        mut meta: ObjectMeta,
    ) -> Result<PutResult> {
        // Derive a deterministic fake ETag from the payload length so callers
        // can verify round-trip integrity without a real checksum.
        let etag = format!("{:x}", data.len());
        meta.content_length = Some(data.len() as u64);

        let mut guard = self.store.write().await;
        guard.insert((bucket.to_string(), key.to_string()), (data, meta));

        Ok(PutResult {
            key: key.to_string(),
            etag: Some(etag),
            version_id: None,
        })
    }

    async fn get_object(&self, bucket: &str, key: &str) -> Result<ObjectData> {
        let guard = self.store.read().await;
        let (data, meta) = guard
            .get(&(bucket.to_string(), key.to_string()))
            .ok_or_else(|| StorageError::NotFound {
                bucket: bucket.to_string(),
                key: key.to_string(),
            })?;

        Ok(ObjectData {
            key: key.to_string(),
            data: data.clone(),
            meta: meta.clone(),
        })
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> Result<()> {
        let mut guard = self.store.write().await;
        guard
            .remove(&(bucket.to_string(), key.to_string()))
            .ok_or_else(|| StorageError::NotFound {
                bucket: bucket.to_string(),
                key: key.to_string(),
            })?;
        Ok(())
    }

    async fn list_objects(
        &self,
        bucket: &str,
        prefix: Option<&str>,
        max: usize,
    ) -> Result<Vec<ObjectListing>> {
        let guard = self.store.read().await;

        let mut entries: Vec<ObjectListing> = guard
            .iter()
            .filter(|((b, k), _)| b == bucket && prefix.map(|p| k.starts_with(p)).unwrap_or(true))
            .map(|((_, k), (data, meta))| ObjectListing {
                key: k.clone(),
                size: data.len() as u64,
                last_modified: meta.last_modified,
                etag: meta.etag.clone(),
            })
            .collect();

        entries.sort_by(|a, b| a.key.cmp(&b.key));
        entries.truncate(max);
        Ok(entries)
    }

    async fn presigned_url(
        &self,
        _bucket: &str,
        _key: &str,
        _ttl: Duration,
        _op: PresignOp,
    ) -> Result<String> {
        Err(StorageError::Unsupported(
            "presigned URLs are not supported by the in-memory provider".to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build test bytes from a static string slice.
    fn make_bytes(s: &str) -> Bytes {
        Bytes::from(s.to_string())
    }

    #[tokio::test]
    async fn test_put_get_roundtrip() {
        let provider = MemoryStoreProvider::new();
        let payload = make_bytes("hello, object store");
        provider
            .put_object(
                "bucket",
                "greet.txt",
                payload.clone(),
                ObjectMeta::default(),
            )
            .await
            .expect("put_object should succeed");

        let retrieved = provider
            .get_object("bucket", "greet.txt")
            .await
            .expect("get_object should succeed");

        assert_eq!(retrieved.data, payload, "round-tripped bytes must match");
        assert_eq!(retrieved.key, "greet.txt");
    }

    #[tokio::test]
    async fn test_list_with_prefix() {
        let provider = MemoryStoreProvider::new();

        // 3 objects under "logs/", 2 under a different prefix
        for i in 0..3_u8 {
            provider
                .put_object(
                    "b",
                    &format!("logs/entry-{}", i),
                    make_bytes("data"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put should succeed");
        }
        for i in 0..2_u8 {
            provider
                .put_object(
                    "b",
                    &format!("metrics/m-{}", i),
                    make_bytes("data"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put should succeed");
        }

        let listed = provider
            .list_objects("b", Some("logs/"), 100)
            .await
            .expect("list should succeed");

        assert_eq!(listed.len(), 3, "only the 3 'logs/' objects should appear");
        assert!(listed.iter().all(|o| o.key.starts_with("logs/")));
    }

    #[tokio::test]
    async fn test_list_max_limits() {
        let provider = MemoryStoreProvider::new();

        for i in 0..10_u8 {
            provider
                .put_object(
                    "bucket",
                    &format!("item-{:02}", i),
                    make_bytes("x"),
                    ObjectMeta::default(),
                )
                .await
                .expect("put should succeed");
        }

        let listed = provider
            .list_objects("bucket", None, 3)
            .await
            .expect("list should succeed");

        assert_eq!(listed.len(), 3, "max=3 must truncate the result set");
    }

    #[tokio::test]
    async fn test_delete_then_get_returns_not_found() {
        let provider = MemoryStoreProvider::new();
        provider
            .put_object("b", "k", make_bytes("v"), ObjectMeta::default())
            .await
            .expect("put should succeed");

        provider
            .delete_object("b", "k")
            .await
            .expect("delete should succeed");

        let err = provider
            .get_object("b", "k")
            .await
            .expect_err("get after delete must return an error");

        assert!(
            matches!(err, StorageError::NotFound { .. }),
            "expected NotFound, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_presigned_url_unsupported() {
        let provider = MemoryStoreProvider::new();
        let err = provider
            .presigned_url("b", "k", Duration::from_secs(300), PresignOp::Get)
            .await
            .expect_err("presigned_url must fail for in-memory provider");

        assert!(
            matches!(err, StorageError::Unsupported(_)),
            "expected Unsupported, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_put_result_has_key() {
        let provider = MemoryStoreProvider::new();
        let result = provider
            .put_object(
                "bucket",
                "my/path/obj.bin",
                make_bytes("abc"),
                ObjectMeta::default(),
            )
            .await
            .expect("put should succeed");

        assert_eq!(
            result.key, "my/path/obj.bin",
            "PutResult.key must match the input key"
        );
    }

    #[tokio::test]
    async fn test_delete_nonexistent_returns_not_found() {
        let provider = MemoryStoreProvider::new();
        let err = provider
            .delete_object("b", "ghost")
            .await
            .expect_err("deleting a non-existent key must error");

        assert!(
            matches!(err, StorageError::NotFound { .. }),
            "expected NotFound, got: {:?}",
            err
        );
    }

    #[tokio::test]
    async fn test_list_empty_bucket() {
        let provider = MemoryStoreProvider::new();
        let listed = provider
            .list_objects("empty-bucket", None, 100)
            .await
            .expect("listing an empty bucket should return Ok");

        assert!(listed.is_empty(), "empty bucket must return an empty list");
    }

    #[tokio::test]
    async fn test_content_length_stored_in_meta() {
        let provider = MemoryStoreProvider::new();
        let payload = make_bytes("hello");
        provider
            .put_object("b", "k", payload, ObjectMeta::default())
            .await
            .expect("put should succeed");

        let obj = provider
            .get_object("b", "k")
            .await
            .expect("get should succeed");
        assert_eq!(
            obj.meta.content_length,
            Some(5),
            "content_length should equal payload size"
        );
    }
}
