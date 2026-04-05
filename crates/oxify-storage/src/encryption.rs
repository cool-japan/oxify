//! Encryption service for secrets
//!
//! Provides AES-256-GCM encryption for secure secret storage

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use oxify_model::EncryptionMetadata;
use rand::RngExt;

/// Encryption service for secrets
pub struct EncryptionService {
    master_key: Vec<u8>,
    key_version: u32,
}

impl EncryptionService {
    /// Create a new encryption service with a master key
    pub fn new(master_key: Vec<u8>) -> Self {
        Self {
            master_key,
            key_version: 1,
        }
    }

    /// Create encryption service from environment variable
    pub fn from_env() -> Result<Self> {
        let key_b64 = std::env::var("OXIFY_MASTER_KEY")
            .context("OXIFY_MASTER_KEY environment variable not set")?;

        let master_key = BASE64
            .decode(key_b64.as_bytes())
            .context("Failed to decode master key from base64")?;

        if master_key.len() != 32 {
            anyhow::bail!("Master key must be 32 bytes (256 bits)");
        }

        Ok(Self {
            master_key,
            key_version: 1,
        })
    }

    /// Generate a random master key (for testing/initialization)
    pub fn generate_master_key() -> Vec<u8> {
        let mut key = vec![0u8; 32];
        rand::rng().fill(&mut key[..]);
        key
    }

    /// Encrypt a plaintext value
    pub fn encrypt(&self, plaintext: &str) -> Result<(Vec<u8>, EncryptionMetadata)> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };

        // Generate random IV (nonce)
        let mut iv = vec![0u8; 12];
        rand::rng().fill(&mut iv[..]);

        // Generate random salt for key derivation
        let mut salt = vec![0u8; 32];
        rand::rng().fill(&mut salt[..]);

        // Derive encryption key from master key and salt using PBKDF2
        let mut derived_key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&self.master_key, &salt, 100_000, &mut derived_key);

        // Create cipher
        let cipher = Aes256Gcm::new(&derived_key.into());
        let nonce = Nonce::from_slice(&iv);

        // Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|e| anyhow::anyhow!("Encryption failed: {e}"))?;

        // Create metadata
        let metadata = EncryptionMetadata {
            algorithm: "AES-256-GCM".to_string(),
            kdf: "PBKDF2-HMAC-SHA256".to_string(),
            salt: BASE64.encode(&salt),
            iv: BASE64.encode(&iv),
            key_version: self.key_version,
        };

        Ok((ciphertext, metadata))
    }

    /// Decrypt a ciphertext value
    pub fn decrypt(&self, ciphertext: &[u8], metadata: &EncryptionMetadata) -> Result<String> {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };

        // Validate algorithm
        if metadata.algorithm != "AES-256-GCM" {
            anyhow::bail!("Unsupported encryption algorithm: {}", metadata.algorithm);
        }

        // Decode salt and IV
        let salt = BASE64
            .decode(metadata.salt.as_bytes())
            .context("Failed to decode salt")?;
        let iv = BASE64
            .decode(metadata.iv.as_bytes())
            .context("Failed to decode IV")?;

        // Derive decryption key
        let mut derived_key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&self.master_key, &salt, 100_000, &mut derived_key);

        // Create cipher
        let cipher = Aes256Gcm::new(&derived_key.into());
        let nonce = Nonce::from_slice(&iv);

        // Decrypt
        let plaintext = cipher
            .decrypt(nonce, ciphertext)
            .map_err(|e| anyhow::anyhow!("Decryption failed: {e}"))?;

        String::from_utf8(plaintext).context("Decrypted value is not valid UTF-8")
    }

    /// Rotate encryption key (re-encrypt with new key version)
    pub fn rotate_key(&mut self, new_master_key: Vec<u8>) {
        self.master_key = new_master_key;
        self.key_version += 1;
    }

    /// Get current key version
    pub fn key_version(&self) -> u32 {
        self.key_version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let master_key = EncryptionService::generate_master_key();
        let service = EncryptionService::new(master_key);

        let plaintext = "my-secret-api-key";
        let (ciphertext, metadata) = service.encrypt(plaintext).unwrap();

        // Verify encrypted value is different
        assert_ne!(ciphertext, plaintext.as_bytes());

        // Decrypt and verify
        let decrypted = service.decrypt(&ciphertext, &metadata).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_ivs() {
        let master_key = EncryptionService::generate_master_key();
        let service = EncryptionService::new(master_key);

        let plaintext = "same-plaintext";

        let (ciphertext1, metadata1) = service.encrypt(plaintext).unwrap();
        let (ciphertext2, metadata2) = service.encrypt(plaintext).unwrap();

        // Different IVs should produce different ciphertexts
        assert_ne!(metadata1.iv, metadata2.iv);
        assert_ne!(ciphertext1, ciphertext2);

        // Both should decrypt correctly
        let decrypted1 = service.decrypt(&ciphertext1, &metadata1).unwrap();
        let decrypted2 = service.decrypt(&ciphertext2, &metadata2).unwrap();

        assert_eq!(decrypted1, plaintext);
        assert_eq!(decrypted2, plaintext);
    }

    #[test]
    fn test_wrong_key_fails() {
        let master_key1 = EncryptionService::generate_master_key();
        let master_key2 = EncryptionService::generate_master_key();

        let service1 = EncryptionService::new(master_key1);
        let service2 = EncryptionService::new(master_key2);

        let plaintext = "secret-data";
        let (ciphertext, metadata) = service1.encrypt(plaintext).unwrap();

        // Decryption with wrong key should fail
        let result = service2.decrypt(&ciphertext, &metadata);
        assert!(result.is_err());
    }
}
