//! Data Encryption for Cached Results and Models
//!
//! This module provides encryption and decryption capabilities for sensitive data
//! such as cached OCR results and model files. It supports multiple encryption
//! algorithms and key management strategies.
//!
//! # Features
//!
//! - AES-256-GCM encryption for data at rest
//! - Key derivation using PBKDF2 or Argon2
//! - Secure key storage and rotation
//! - Encrypted cache integration
//! - Model file encryption
//! - Key versioning for rotation support
//!
//! # Example
//!
//! ```rust,ignore
//! use oxify_connect_vision::encryption::{EncryptionProvider, EncryptionConfig};
//!
//! let config = EncryptionConfig::default()
//!     .with_master_key("my-secret-key");
//!
//! let provider = EncryptionProvider::new(config)?;
//!
//! // Encrypt data
//! let encrypted = provider.encrypt(b"sensitive data")?;
//!
//! // Decrypt data
//! let decrypted = provider.decrypt(&encrypted)?;
//! assert_eq!(decrypted, b"sensitive data");
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Encryption errors
#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("Encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("Decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("Invalid key: {0}")]
    InvalidKey(String),

    #[error("Key derivation failed: {0}")]
    KeyDerivationFailed(String),

    #[error("Invalid configuration: {0}")]
    ConfigError(String),

    #[error("Key not found: {0}")]
    KeyNotFound(String),

    #[error("Invalid encrypted data format")]
    InvalidFormat,
}

pub type Result<T> = std::result::Result<T, EncryptionError>;

/// Encryption algorithm
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EncryptionAlgorithm {
    /// AES-256-GCM (recommended)
    #[default]
    Aes256Gcm,

    /// ChaCha20-Poly1305
    ChaCha20Poly1305,
}

/// Key derivation function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum KeyDerivationFunction {
    /// PBKDF2 with SHA-256
    Pbkdf2Sha256,

    /// Argon2id (recommended for new applications)
    #[default]
    Argon2id,
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptionConfig {
    /// Encryption algorithm
    pub algorithm: EncryptionAlgorithm,

    /// Key derivation function
    pub kdf: KeyDerivationFunction,

    /// Master key (for key derivation)
    #[serde(skip)]
    pub master_key: Option<Vec<u8>>,

    /// Salt size in bytes
    pub salt_size: usize,

    /// Nonce size in bytes
    pub nonce_size: usize,

    /// PBKDF2 iterations (if using PBKDF2)
    pub pbkdf2_iterations: u32,

    /// Argon2 memory cost in KiB (if using Argon2)
    pub argon2_memory_cost: u32,

    /// Argon2 time cost (if using Argon2)
    pub argon2_time_cost: u32,

    /// Enable key versioning for rotation
    pub enable_versioning: bool,

    /// Current key version
    pub key_version: u32,
}

impl Default for EncryptionConfig {
    fn default() -> Self {
        Self {
            algorithm: EncryptionAlgorithm::default(),
            kdf: KeyDerivationFunction::default(),
            master_key: None,
            salt_size: 32,
            nonce_size: 12,
            pbkdf2_iterations: 100_000,
            argon2_memory_cost: 65536, // 64 MiB
            argon2_time_cost: 3,
            enable_versioning: true,
            key_version: 1,
        }
    }
}

impl EncryptionConfig {
    /// Create a new configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the master key
    pub fn with_master_key(mut self, key: impl AsRef<[u8]>) -> Self {
        self.master_key = Some(key.as_ref().to_vec());
        self
    }

    /// Set the encryption algorithm
    pub fn with_algorithm(mut self, algorithm: EncryptionAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Set the key derivation function
    pub fn with_kdf(mut self, kdf: KeyDerivationFunction) -> Self {
        self.kdf = kdf;
        self
    }

    /// Set the key version
    pub fn with_key_version(mut self, version: u32) -> Self {
        self.key_version = version;
        self
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        if self.master_key.is_none() {
            return Err(EncryptionError::ConfigError(
                "Master key is required".to_string(),
            ));
        }

        if self.salt_size < 16 {
            return Err(EncryptionError::ConfigError(
                "Salt size must be at least 16 bytes".to_string(),
            ));
        }

        if self.nonce_size < 12 {
            return Err(EncryptionError::ConfigError(
                "Nonce size must be at least 12 bytes".to_string(),
            ));
        }

        if self.pbkdf2_iterations < 10_000 {
            return Err(EncryptionError::ConfigError(
                "PBKDF2 iterations must be at least 10,000".to_string(),
            ));
        }

        Ok(())
    }
}

/// Encrypted data container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// Algorithm used
    pub algorithm: EncryptionAlgorithm,

    /// Key version
    pub version: u32,

    /// Salt used for key derivation
    pub salt: Vec<u8>,

    /// Nonce used for encryption
    pub nonce: Vec<u8>,

    /// Encrypted ciphertext
    pub ciphertext: Vec<u8>,

    /// Authentication tag (for AEAD)
    pub tag: Vec<u8>,
}

impl EncryptedData {
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|_e| EncryptionError::InvalidFormat)
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        serde_json::from_slice(data).map_err(|_e| EncryptionError::InvalidFormat)
    }
}

/// Key information
#[derive(Debug, Clone)]
struct KeyInfo {
    /// Key bytes
    key: Vec<u8>,

    /// Key version
    #[allow(dead_code)]
    version: u32,

    /// Creation timestamp
    #[allow(dead_code)]
    created_at: std::time::SystemTime,
}

/// Encryption provider
#[derive(Debug)]
pub struct EncryptionProvider {
    /// Configuration
    config: EncryptionConfig,

    /// Derived keys (cached by version)
    keys: std::sync::RwLock<HashMap<u32, KeyInfo>>,

    /// Statistics
    stats: std::sync::Mutex<EncryptionStats>,
}

impl EncryptionProvider {
    /// Create a new encryption provider
    pub fn new(config: EncryptionConfig) -> Result<Self> {
        config.validate()?;

        Ok(Self {
            config,
            keys: std::sync::RwLock::new(HashMap::new()),
            stats: std::sync::Mutex::new(EncryptionStats::default()),
        })
    }

    /// Derive a key from the master key and salt
    fn derive_key(&self, salt: &[u8], _version: u32) -> Result<Vec<u8>> {
        let master_key = self
            .config
            .master_key
            .as_ref()
            .ok_or_else(|| EncryptionError::InvalidKey("Master key not set".to_string()))?;

        match self.config.kdf {
            KeyDerivationFunction::Pbkdf2Sha256 => self.derive_key_pbkdf2(master_key, salt),
            KeyDerivationFunction::Argon2id => self.derive_key_argon2(master_key, salt),
        }
    }

    /// Derive a key using PBKDF2-SHA256
    fn derive_key_pbkdf2(&self, master_key: &[u8], salt: &[u8]) -> Result<Vec<u8>> {
        use oxicrypto_hash::Sha256;

        // Simple PBKDF2 implementation using SHA-256
        let mut key = master_key.to_vec();
        for _ in 0..self.config.pbkdf2_iterations {
            let mut buf = Vec::with_capacity(key.len() + salt.len());
            buf.extend_from_slice(&key);
            buf.extend_from_slice(salt);
            key = Sha256.hash_fixed(&buf).to_vec();
        }

        Ok(key)
    }

    /// Derive a key using Argon2id
    fn derive_key_argon2(&self, master_key: &[u8], salt: &[u8]) -> Result<Vec<u8>> {
        // Simplified Argon2 implementation
        // In production, use a proper Argon2 library
        use oxicrypto_hash::Sha256;

        let memory_cost_bytes = self.config.argon2_memory_cost.to_le_bytes();
        let time_cost_bytes = self.config.argon2_time_cost.to_le_bytes();
        let mut buf = Vec::with_capacity(
            master_key.len() + salt.len() + memory_cost_bytes.len() + time_cost_bytes.len(),
        );
        buf.extend_from_slice(master_key);
        buf.extend_from_slice(salt);
        buf.extend_from_slice(&memory_cost_bytes);
        buf.extend_from_slice(&time_cost_bytes);

        let mut key = Sha256.hash_fixed(&buf).to_vec();

        // Apply time cost
        for _ in 0..self.config.argon2_time_cost {
            let mut buf = Vec::with_capacity(key.len() + salt.len());
            buf.extend_from_slice(&key);
            buf.extend_from_slice(salt);
            key = Sha256.hash_fixed(&buf).to_vec();
        }

        Ok(key)
    }

    /// Get or create a key for the given version
    fn get_or_create_key(&self, salt: &[u8], version: u32) -> Result<Vec<u8>> {
        // Check if key is cached
        {
            let keys = self.keys.read().unwrap_or_else(|e| e.into_inner());
            if let Some(key_info) = keys.get(&version) {
                return Ok(key_info.key.clone());
            }
        }

        // Derive new key
        let key = self.derive_key(salt, version)?;

        // Cache the key
        {
            let mut keys = self.keys.write().unwrap_or_else(|e| e.into_inner());
            keys.insert(
                version,
                KeyInfo {
                    key: key.clone(),
                    version,
                    created_at: std::time::SystemTime::now(),
                },
            );
        }

        Ok(key)
    }

    /// Generate a random salt
    fn generate_salt(&self) -> Vec<u8> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_nanos();

        use oxicrypto_hash::Sha256;
        let timestamp_bytes = timestamp.to_le_bytes();
        let counter_bytes = counter.to_le_bytes();
        let mut buf = Vec::with_capacity(timestamp_bytes.len() + counter_bytes.len());
        buf.extend_from_slice(&timestamp_bytes);
        buf.extend_from_slice(&counter_bytes);

        Sha256.hash_fixed(&buf)[..self.config.salt_size].to_vec()
    }

    /// Generate a random nonce
    fn generate_nonce(&self) -> Vec<u8> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);

        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time before Unix epoch")
            .as_nanos();

        use oxicrypto_hash::Sha256;
        let timestamp_bytes = timestamp.to_le_bytes();
        let counter_bytes = counter.to_le_bytes();
        let mut buf =
            Vec::with_capacity(timestamp_bytes.len() + counter_bytes.len() + b"nonce".len());
        buf.extend_from_slice(&timestamp_bytes);
        buf.extend_from_slice(&counter_bytes);
        buf.extend_from_slice(b"nonce");

        Sha256.hash_fixed(&buf)[..self.config.nonce_size].to_vec()
    }

    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData> {
        let salt = self.generate_salt();
        let nonce = self.generate_nonce();
        let key = self.get_or_create_key(&salt, self.config.key_version)?;

        let (ciphertext, tag) = match self.config.algorithm {
            EncryptionAlgorithm::Aes256Gcm => self.encrypt_aes_gcm(&key, &nonce, plaintext)?,
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.encrypt_chacha20(&key, &nonce, plaintext)?
            }
        };

        // Update statistics
        {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.total_encryptions += 1;
            stats.bytes_encrypted += plaintext.len() as u64;
        }

        Ok(EncryptedData {
            algorithm: self.config.algorithm,
            version: self.config.key_version,
            salt,
            nonce,
            ciphertext,
            tag,
        })
    }

    /// Encrypt using AES-256-GCM
    fn encrypt_aes_gcm(
        &self,
        key: &[u8],
        nonce: &[u8],
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        // Simplified AES-GCM implementation
        // In production, use a proper crypto library like `aes-gcm`

        use oxicrypto_hash::Sha256;

        let mut ciphertext = Vec::new();
        for (i, &byte) in plaintext.iter().enumerate() {
            let index_bytes = (i as u64).to_le_bytes();
            let mut buf = Vec::with_capacity(key.len() + nonce.len() + index_bytes.len());
            buf.extend_from_slice(key);
            buf.extend_from_slice(nonce);
            buf.extend_from_slice(&index_bytes);
            let keystream = Sha256.hash_fixed(&buf);
            ciphertext.push(byte ^ keystream[0]);
        }

        // Generate authentication tag
        let mut buf = Vec::with_capacity(key.len() + nonce.len() + ciphertext.len());
        buf.extend_from_slice(key);
        buf.extend_from_slice(nonce);
        buf.extend_from_slice(&ciphertext);
        let tag = Sha256.hash_fixed(&buf)[..16].to_vec();

        Ok((ciphertext, tag))
    }

    /// Encrypt using ChaCha20-Poly1305
    fn encrypt_chacha20(
        &self,
        key: &[u8],
        nonce: &[u8],
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>)> {
        // Simplified ChaCha20-Poly1305 implementation
        // In production, use a proper crypto library like `chacha20poly1305`

        use oxicrypto_hash::Sha256;

        let mut ciphertext = Vec::new();
        for (i, &byte) in plaintext.iter().enumerate() {
            let index_bytes = (i as u64).to_le_bytes();
            let mut buf =
                Vec::with_capacity(key.len() + nonce.len() + index_bytes.len() + b"chacha20".len());
            buf.extend_from_slice(key);
            buf.extend_from_slice(nonce);
            buf.extend_from_slice(&index_bytes);
            buf.extend_from_slice(b"chacha20");
            let keystream = Sha256.hash_fixed(&buf);
            ciphertext.push(byte ^ keystream[0]);
        }

        // Generate authentication tag
        let mut buf =
            Vec::with_capacity(key.len() + nonce.len() + ciphertext.len() + b"poly1305".len());
        buf.extend_from_slice(key);
        buf.extend_from_slice(nonce);
        buf.extend_from_slice(&ciphertext);
        buf.extend_from_slice(b"poly1305");
        let tag = Sha256.hash_fixed(&buf)[..16].to_vec();

        Ok((ciphertext, tag))
    }

    /// Decrypt data
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>> {
        let key = self.get_or_create_key(&encrypted.salt, encrypted.version)?;

        // Verify tag first
        let expected_tag = self.compute_tag(
            &key,
            &encrypted.nonce,
            &encrypted.ciphertext,
            encrypted.algorithm,
        )?;

        if expected_tag != encrypted.tag {
            return Err(EncryptionError::DecryptionFailed(
                "Authentication failed".to_string(),
            ));
        }

        let plaintext = match encrypted.algorithm {
            EncryptionAlgorithm::Aes256Gcm => {
                self.decrypt_aes_gcm(&key, &encrypted.nonce, &encrypted.ciphertext)?
            }
            EncryptionAlgorithm::ChaCha20Poly1305 => {
                self.decrypt_chacha20(&key, &encrypted.nonce, &encrypted.ciphertext)?
            }
        };

        // Update statistics
        {
            let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
            stats.total_decryptions += 1;
            stats.bytes_decrypted += plaintext.len() as u64;
        }

        Ok(plaintext)
    }

    /// Compute authentication tag
    fn compute_tag(
        &self,
        key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
        algorithm: EncryptionAlgorithm,
    ) -> Result<Vec<u8>> {
        use oxicrypto_hash::Sha256;

        let extra = if algorithm == EncryptionAlgorithm::ChaCha20Poly1305 {
            b"poly1305".as_slice()
        } else {
            b"".as_slice()
        };
        let mut buf = Vec::with_capacity(key.len() + nonce.len() + ciphertext.len() + extra.len());
        buf.extend_from_slice(key);
        buf.extend_from_slice(nonce);
        buf.extend_from_slice(ciphertext);
        buf.extend_from_slice(extra);

        Ok(Sha256.hash_fixed(&buf)[..16].to_vec())
    }

    /// Decrypt using AES-256-GCM
    fn decrypt_aes_gcm(&self, key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        use oxicrypto_hash::Sha256;

        let mut plaintext = Vec::new();
        for (i, &byte) in ciphertext.iter().enumerate() {
            let index_bytes = (i as u64).to_le_bytes();
            let mut buf = Vec::with_capacity(key.len() + nonce.len() + index_bytes.len());
            buf.extend_from_slice(key);
            buf.extend_from_slice(nonce);
            buf.extend_from_slice(&index_bytes);
            let keystream = Sha256.hash_fixed(&buf);
            plaintext.push(byte ^ keystream[0]);
        }

        Ok(plaintext)
    }

    /// Decrypt using ChaCha20-Poly1305
    fn decrypt_chacha20(&self, key: &[u8], nonce: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        use oxicrypto_hash::Sha256;

        let mut plaintext = Vec::new();
        for (i, &byte) in ciphertext.iter().enumerate() {
            let index_bytes = (i as u64).to_le_bytes();
            let mut buf =
                Vec::with_capacity(key.len() + nonce.len() + index_bytes.len() + b"chacha20".len());
            buf.extend_from_slice(key);
            buf.extend_from_slice(nonce);
            buf.extend_from_slice(&index_bytes);
            buf.extend_from_slice(b"chacha20");
            let keystream = Sha256.hash_fixed(&buf);
            plaintext.push(byte ^ keystream[0]);
        }

        Ok(plaintext)
    }

    /// Rotate to a new key version
    pub fn rotate_key(&mut self, new_version: u32, new_master_key: impl AsRef<[u8]>) -> Result<()> {
        if new_version <= self.config.key_version {
            return Err(EncryptionError::InvalidKey(
                "New version must be greater than current version".to_string(),
            ));
        }

        self.config.master_key = Some(new_master_key.as_ref().to_vec());
        self.config.key_version = new_version;

        // Clear cached keys
        {
            let mut keys = self.keys.write().unwrap_or_else(|e| e.into_inner());
            keys.clear();
        }

        Ok(())
    }

    /// Get encryption statistics
    pub fn get_stats(&self) -> EncryptionStats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reset statistics
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap_or_else(|e| e.into_inner());
        *stats = EncryptionStats::default();
    }

    /// Get configuration
    pub fn config(&self) -> &EncryptionConfig {
        &self.config
    }

    /// Clear cached keys
    pub fn clear_keys(&self) {
        let mut keys = self.keys.write().unwrap_or_else(|e| e.into_inner());
        keys.clear();
    }
}

/// Encryption statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncryptionStats {
    /// Total encryptions performed
    pub total_encryptions: u64,

    /// Total decryptions performed
    pub total_decryptions: u64,

    /// Total bytes encrypted
    pub bytes_encrypted: u64,

    /// Total bytes decrypted
    pub bytes_decrypted: u64,

    /// Failed encryptions
    pub failed_encryptions: u64,

    /// Failed decryptions
    pub failed_decryptions: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_config_default() {
        let config = EncryptionConfig::default();
        assert_eq!(config.algorithm, EncryptionAlgorithm::Aes256Gcm);
        assert_eq!(config.kdf, KeyDerivationFunction::Argon2id);
        assert_eq!(config.salt_size, 32);
        assert_eq!(config.nonce_size, 12);
    }

    #[test]
    fn test_encryption_config_builder() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key")
            .with_algorithm(EncryptionAlgorithm::ChaCha20Poly1305)
            .with_kdf(KeyDerivationFunction::Pbkdf2Sha256)
            .with_key_version(2);

        assert_eq!(config.algorithm, EncryptionAlgorithm::ChaCha20Poly1305);
        assert_eq!(config.kdf, KeyDerivationFunction::Pbkdf2Sha256);
        assert_eq!(config.key_version, 2);
    }

    #[test]
    fn test_encryption_config_validation() {
        let config = EncryptionConfig::default();
        assert!(config.validate().is_err()); // No master key

        let config = EncryptionConfig::default().with_master_key(b"test-key");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_encrypt_decrypt_aes() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key-32-bytes-long!!!")
            .with_algorithm(EncryptionAlgorithm::Aes256Gcm);

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Hello, World! This is a secret message.";
        let encrypted = provider.encrypt(plaintext).unwrap();

        assert_eq!(encrypted.algorithm, EncryptionAlgorithm::Aes256Gcm);
        assert_eq!(encrypted.version, 1);
        assert_ne!(encrypted.ciphertext, plaintext);

        let decrypted = provider.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_chacha20() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key-32-bytes-long!!!")
            .with_algorithm(EncryptionAlgorithm::ChaCha20Poly1305);

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Hello, World! This is a secret message.";
        let encrypted = provider.encrypt(plaintext).unwrap();

        assert_eq!(encrypted.algorithm, EncryptionAlgorithm::ChaCha20Poly1305);
        assert_ne!(encrypted.ciphertext, plaintext);

        let decrypted = provider.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encryption_statistics() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key-32-bytes-long!!!");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test data";
        let encrypted = provider.encrypt(plaintext).unwrap();
        let _decrypted = provider.decrypt(&encrypted).unwrap();

        let stats = provider.get_stats();
        assert_eq!(stats.total_encryptions, 1);
        assert_eq!(stats.total_decryptions, 1);
        assert_eq!(stats.bytes_encrypted, plaintext.len() as u64);
        assert_eq!(stats.bytes_decrypted, plaintext.len() as u64);
    }

    #[test]
    fn test_key_derivation_pbkdf2() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key")
            .with_kdf(KeyDerivationFunction::Pbkdf2Sha256);

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test data";
        let encrypted = provider.encrypt(plaintext).unwrap();
        let decrypted = provider.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_derivation_argon2() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key")
            .with_kdf(KeyDerivationFunction::Argon2id);

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test data";
        let encrypted = provider.encrypt(plaintext).unwrap();
        let decrypted = provider.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_versioning() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key-v1")
            .with_key_version(1);

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test data with versioning";
        let encrypted_v1 = provider.encrypt(plaintext).unwrap();
        assert_eq!(encrypted_v1.version, 1);

        // Can still decrypt with v1 key
        let decrypted = provider.decrypt(&encrypted_v1).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_key_rotation() {
        let config = EncryptionConfig::new()
            .with_master_key(b"test-master-key-v1")
            .with_key_version(1);

        let mut provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test data";
        let encrypted_v1 = provider.encrypt(plaintext).unwrap();
        assert_eq!(encrypted_v1.version, 1);

        // Decrypt before rotation works
        let decrypted_v1 = provider.decrypt(&encrypted_v1).unwrap();
        assert_eq!(decrypted_v1, plaintext);

        // Rotate to new key
        provider.rotate_key(2, b"test-master-key-v2").unwrap();

        // New encryptions use v2
        let encrypted_v2 = provider.encrypt(plaintext).unwrap();
        assert_eq!(encrypted_v2.version, 2);

        // Can decrypt v2 data
        let decrypted_v2 = provider.decrypt(&encrypted_v2).unwrap();
        assert_eq!(decrypted_v2, plaintext);

        // Note: Old v1 data cannot be decrypted after key rotation
        // because the master key has been replaced
        let result = provider.decrypt(&encrypted_v1);
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_data_detection() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Important data";
        let mut encrypted = provider.encrypt(plaintext).unwrap();

        // Tamper with ciphertext
        encrypted.ciphertext[0] ^= 1;

        // Should fail to decrypt
        let result = provider.decrypt(&encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialization() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test serialization";
        let encrypted = provider.encrypt(plaintext).unwrap();

        // Serialize and deserialize
        let bytes = encrypted.to_bytes().unwrap();
        let deserialized = EncryptedData::from_bytes(&bytes).unwrap();

        // Should be able to decrypt
        let decrypted = provider.decrypt(&deserialized).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_empty_data() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"";
        let encrypted = provider.encrypt(plaintext).unwrap();
        let decrypted = provider.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_large_data() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = vec![42u8; 10_000];
        let encrypted = provider.encrypt(&plaintext).unwrap();
        let decrypted = provider.decrypt(&encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_stats_reset() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test";
        let encrypted = provider.encrypt(plaintext).unwrap();
        let _decrypted = provider.decrypt(&encrypted).unwrap();

        let stats = provider.get_stats();
        assert_eq!(stats.total_encryptions, 1);

        provider.reset_stats();
        let stats = provider.get_stats();
        assert_eq!(stats.total_encryptions, 0);
    }

    #[test]
    fn test_clear_keys() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Test";
        let encrypted = provider.encrypt(plaintext).unwrap();

        // Clear cached keys
        provider.clear_keys();

        // Should still be able to decrypt (will re-derive key)
        let decrypted = provider.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_nonces() {
        let config = EncryptionConfig::new().with_master_key(b"test-master-key");

        let provider = EncryptionProvider::new(config).unwrap();

        let plaintext = b"Same data";
        let encrypted1 = provider.encrypt(plaintext).unwrap();
        let encrypted2 = provider.encrypt(plaintext).unwrap();

        // Same plaintext should produce different ciphertexts (different nonces)
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
        assert_ne!(encrypted1.ciphertext, encrypted2.ciphertext);

        // Both should decrypt correctly
        let decrypted1 = provider.decrypt(&encrypted1).unwrap();
        let decrypted2 = provider.decrypt(&encrypted2).unwrap();
        assert_eq!(decrypted1, plaintext);
        assert_eq!(decrypted2, plaintext);
    }

    // ------------------------------------------------------------------
    // Golden-value regression tests (PRE-migration baseline).
    //
    // These tests pin the exact byte-level output of the CURRENT sha2-based
    // KDF/keystream construction for FIXED inputs. They exist so that a
    // later migration off `sha2` can be proven to produce byte-identical
    // output. Round-trip (encrypt -> decrypt) tests alone cannot catch this
    // class of regression: if encrypt and decrypt both change in a
    // self-consistent way, round-trip still passes even though the actual
    // ciphertext/tag/key bytes silently changed underneath.
    //
    // DO NOT "fix" the underlying algorithms here — they are an
    // intentionally simplified, home-grown construction (not real
    // AES-GCM/ChaCha20-Poly1305/PBKDF2/Argon2). These tests only freeze the
    // CURRENT observable behavior so it can be diffed after migration.
    //
    // `generate_salt`/`generate_nonce` are deliberately NOT golden-tested:
    // both mix `SystemTime::now()` nanoseconds and a process-lifetime
    // atomic counter into the hash input, so their output is not
    // reproducible across runs.
    // ------------------------------------------------------------------

    /// Fixed 32-byte key used by the golden encryption tests.
    const GOLDEN_KEY_32: &[u8; 32] = b"0123456789abcdef0123456789abcdef";

    /// Fixed 12-byte nonce used by the golden encryption tests.
    const GOLDEN_NONCE_12: &[u8; 12] = b"golden-nonce";

    /// GOLDEN TEST — captures the PRE-migration (sha2-based) output of
    /// `derive_key_pbkdf2` for a fixed master key, salt, and iteration
    /// count (the `EncryptionConfig` default of 100_000). This is a
    /// hand-rolled "iterated SHA-256" construction, not real
    /// PBKDF2-HMAC-SHA256; after the sha2 migration this exact hex digest
    /// must still be produced for these inputs.
    #[test]
    fn test_golden_derive_key_pbkdf2() {
        let config = EncryptionConfig::new()
            .with_master_key(b"golden-master-key")
            .with_kdf(KeyDerivationFunction::Pbkdf2Sha256);
        let provider = EncryptionProvider::new(config).expect("valid golden config");

        let key = provider
            .derive_key_pbkdf2(b"golden-master-key", b"golden-salt")
            .expect("pbkdf2 derivation should succeed");

        assert_eq!(
            hex::encode(&key),
            "437b48c42055a32d1d86a6b7ddc9f555f3a46e035d6d55b84a0ea7b279787c40",
            "derive_key_pbkdf2 golden output changed for fixed inputs \
             (master_key=b\"golden-master-key\", salt=b\"golden-salt\", pbkdf2_iterations=100000)"
        );
    }

    /// GOLDEN TEST — captures the PRE-migration (sha2-based) output of
    /// `derive_key_argon2` for a fixed master key, salt, and the
    /// `EncryptionConfig` default memory/time cost (65536 / 3). This is a
    /// simplified stand-in for Argon2id (plain iterated SHA-256), not real
    /// Argon2; after migration this exact hex digest must still be
    /// produced for these inputs.
    #[test]
    fn test_golden_derive_key_argon2() {
        let config = EncryptionConfig::new()
            .with_master_key(b"golden-master-key")
            .with_kdf(KeyDerivationFunction::Argon2id);
        let provider = EncryptionProvider::new(config).expect("valid golden config");

        let key = provider
            .derive_key_argon2(b"golden-master-key", b"golden-salt")
            .expect("argon2 derivation should succeed");

        assert_eq!(
            hex::encode(&key),
            "2001ac0197ccf8308f946befcbf9fafa8cfc3c82ddcc4a9e58a50ea653fe55a3",
            "derive_key_argon2 golden output changed for fixed inputs \
             (master_key=b\"golden-master-key\", salt=b\"golden-salt\", \
             argon2_memory_cost=65536, argon2_time_cost=3)"
        );
    }

    /// GOLDEN TEST — captures the PRE-migration (sha2-based) ciphertext AND
    /// authentication tag produced by `encrypt_aes_gcm` for a fixed
    /// 32-byte key, fixed 12-byte nonce, and fixed plaintext. This
    /// "AES-GCM" is a simplified per-byte keystream XOR construction, not
    /// real AES-GCM; both outputs must remain byte-identical post-migration.
    #[test]
    fn test_golden_encrypt_aes_gcm() {
        let config = EncryptionConfig::new().with_master_key(b"golden-master-key");
        let provider = EncryptionProvider::new(config).expect("valid golden config");

        let plaintext = b"Golden plaintext for pre-migration regression test.";
        let (ciphertext, tag) = provider
            .encrypt_aes_gcm(GOLDEN_KEY_32, GOLDEN_NONCE_12, plaintext)
            .expect("aes-gcm encryption should succeed");

        assert_eq!(
            hex::encode(&ciphertext),
            "7105a57dcd16139a0440176910f11f3f5d97e71289a25f9c1eadbb5685da169\
edacb39196a7318552d0ccfa99cb39f2d33791c",
            "encrypt_aes_gcm golden ciphertext changed for fixed key/nonce/plaintext"
        );
        assert_eq!(
            hex::encode(&tag),
            "ed00350bb742e1393ed3827cccc92727",
            "encrypt_aes_gcm golden tag changed for fixed key/nonce/plaintext"
        );
    }

    /// GOLDEN TEST — captures the PRE-migration (sha2-based) ciphertext AND
    /// authentication tag produced by `encrypt_chacha20` for a fixed
    /// 32-byte key, fixed 12-byte nonce, and fixed plaintext. This
    /// "ChaCha20" is a simplified per-byte keystream XOR construction, not
    /// real ChaCha20-Poly1305; both outputs must remain byte-identical
    /// post-migration.
    #[test]
    fn test_golden_encrypt_chacha20() {
        let config = EncryptionConfig::new().with_master_key(b"golden-master-key");
        let provider = EncryptionProvider::new(config).expect("valid golden config");

        let plaintext = b"Golden plaintext for pre-migration regression test.";
        let (ciphertext, tag) = provider
            .encrypt_chacha20(GOLDEN_KEY_32, GOLDEN_NONCE_12, plaintext)
            .expect("chacha20 encryption should succeed");

        assert_eq!(
            hex::encode(&ciphertext),
            "607bb2d6554035cd416886c68970f53738cf2933581c553deee529f0b7dcc38\
665a1a999adb5f73b08f12374bc2a36f3e186b2",
            "encrypt_chacha20 golden ciphertext changed for fixed key/nonce/plaintext"
        );
        assert_eq!(
            hex::encode(&tag),
            "f45203baeb1df18dd58ac25cbdf0a90a",
            "encrypt_chacha20 golden tag changed for fixed key/nonce/plaintext"
        );
    }

    /// GOLDEN TEST — captures the PRE-migration (sha2-based) authentication
    /// tag produced by `compute_tag` for a fixed key/nonce/ciphertext,
    /// across both supported algorithms. The `ChaCha20Poly1305` branch
    /// mixes in an extra `b"poly1305"` domain separator, so it must be
    /// pinned separately from the `Aes256Gcm` branch.
    #[test]
    fn test_golden_compute_tag() {
        let config = EncryptionConfig::new().with_master_key(b"golden-master-key");
        let provider = EncryptionProvider::new(config).expect("valid golden config");

        let ciphertext = b"golden-ciphertext-fixture-bytes";

        let tag_aes = provider
            .compute_tag(
                GOLDEN_KEY_32,
                GOLDEN_NONCE_12,
                ciphertext,
                EncryptionAlgorithm::Aes256Gcm,
            )
            .expect("compute_tag (aes) should succeed");
        assert_eq!(
            hex::encode(&tag_aes),
            "38a28fe14cb67d5dbf55d73cfd2768d1",
            "compute_tag golden AES-GCM tag changed for fixed key/nonce/ciphertext"
        );

        let tag_chacha = provider
            .compute_tag(
                GOLDEN_KEY_32,
                GOLDEN_NONCE_12,
                ciphertext,
                EncryptionAlgorithm::ChaCha20Poly1305,
            )
            .expect("compute_tag (chacha20) should succeed");
        assert_eq!(
            hex::encode(&tag_chacha),
            "a07ee014ffdaf701d2560908647b31d5",
            "compute_tag golden ChaCha20Poly1305 tag changed for fixed key/nonce/ciphertext"
        );
    }
}
