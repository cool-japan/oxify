use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Metadata associated with a stored object.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ObjectMeta {
    /// MIME content-type of the object, e.g. `"application/octet-stream"`.
    pub content_type: Option<String>,
    /// Size of the object in bytes, as reported by the provider.
    pub content_length: Option<u64>,
    /// Entity tag (opaque identifier) returned by the provider.
    pub etag: Option<String>,
    /// Timestamp of the last write operation.
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    /// Arbitrary user-defined key/value pairs stored alongside the object.
    pub metadata: HashMap<String, String>,
}

/// A fully retrieved object, including its raw bytes and metadata.
#[derive(Debug, Clone)]
pub struct ObjectData {
    /// The storage key this object was retrieved from.
    pub key: String,
    /// The raw bytes of the object body.
    pub data: Bytes,
    /// Metadata returned by the provider.
    pub meta: ObjectMeta,
}

/// The result of a successful put (upload) operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutResult {
    /// The storage key the object was written to.
    pub key: String,
    /// Entity tag assigned by the provider (if any).
    pub etag: Option<String>,
    /// Version identifier assigned by the provider (if versioning is enabled).
    pub version_id: Option<String>,
}

/// Summary information for a single object returned by a list operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectListing {
    /// The full storage key of the object.
    pub key: String,
    /// Size of the object in bytes.
    pub size: u64,
    /// Timestamp of the last write operation (if known).
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    /// Entity tag (if returned by the provider).
    pub etag: Option<String>,
}

/// The HTTP verb that a presigned URL should be scoped to.
#[derive(Debug, Clone, Copy)]
pub enum PresignOp {
    /// Allow the bearer to download the object.
    Get,
    /// Allow the bearer to upload to the key.
    Put,
}
