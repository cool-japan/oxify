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

    /// Derive a 32-byte encryption key from the master key and a per-secret salt.
    ///
    /// Uses PBKDF2-HMAC-SHA256 with 100_000 iterations. This is the single
    /// SHA-256-based key-derivation seam shared by [`Self::encrypt`] and
    /// [`Self::decrypt`]. It is deliberately isolated so that golden regression
    /// tests can capture its exact byte output prior to migrating the SHA-256
    /// backend, allowing that migration to be proven byte-identical.
    fn derive_key(&self, salt: &[u8]) -> [u8; 32] {
        let mut derived_key = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(&self.master_key, salt, 100_000, &mut derived_key);
        derived_key
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
        let derived_key = self.derive_key(&salt);

        // Create cipher
        let cipher = Aes256Gcm::new(&derived_key.into());
        let iv_arr: [u8; 12] = iv
            .as_slice()
            .try_into()
            .context("IV must be exactly 12 bytes")?;
        let nonce = Nonce::from(iv_arr);

        // Encrypt
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
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
        let derived_key = self.derive_key(&salt);

        // Create cipher
        let cipher = Aes256Gcm::new(&derived_key.into());
        let iv_arr: [u8; 12] = iv
            .as_slice()
            .try_into()
            .context("IV must be exactly 12 bytes")?;
        let nonce = Nonce::from(iv_arr);

        // Decrypt
        let plaintext = cipher
            .decrypt(&nonce, ciphertext)
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

    /// Golden regression test: captures the exact PRE-migration (sha2-based)
    /// PBKDF2-HMAC-SHA256 key-derivation output for a fixed master key and a
    /// fixed salt. [`EncryptionService::derive_key`] derives the AES key via
    /// `pbkdf2::pbkdf2_hmac::<sha2::Sha256>` with 100_000 iterations; this is
    /// the only SHA-256-based construction in this file. After the SHA-256
    /// backend is migrated, this literal MUST remain byte-identical, proving
    /// the migration did not alter derived-key bytes. This is a fixed-input /
    /// fixed-output guard (NOT a self-consistent round trip).
    #[test]
    fn test_golden_derive_key_pbkdf2() {
        let service = EncryptionService::new(b"golden-master-key".to_vec());
        let derived_key = service.derive_key(b"golden-salt");
        assert_eq!(
            hex::encode(derived_key),
            "ac979c2a69312101eb577afe3b1f197ca4941336c5c3c7656c1bb7ffabfd8316",
            "PBKDF2-HMAC-SHA256 derived key must match the pre-migration golden value"
        );
    }

    /// Golden regression test: captures the exact PRE-migration AES-256-GCM
    /// ciphertext and authentication tag for a fixed 32-byte key, a fixed
    /// 12-byte nonce, and a fixed plaintext. This mirrors the cipher
    /// construction used by [`EncryptionService::encrypt`] and locks the
    /// primitive against any accidental behavioral change during the SHA-256
    /// migration. (AES-256-GCM itself is not SHA-256-based, so it is expected
    /// to be unaffected; this guard makes that expectation enforceable.)
    #[test]
    fn test_golden_encrypt_aes_gcm() {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        let key = *b"0123456789abcdef0123456789abcdef";
        let nonce_bytes = *b"golden-nonce";
        let plaintext = b"golden-plaintext-value";
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from(nonce_bytes);
        let out = cipher
            .encrypt(&nonce, plaintext.as_ref())
            .expect("aes-gcm encryption of fixed golden input must succeed");
        let (ciphertext, tag) = out.split_at(out.len() - 16);
        assert_eq!(
            hex::encode(ciphertext),
            "6329becba284eaa9fa4ab0b2321a3b51ab4075b9e669",
            "AES-256-GCM ciphertext must match the pre-migration golden value"
        );
        assert_eq!(
            hex::encode(tag),
            "9cc1f50490e6f90c1eb2b0927c18edb9",
            "AES-256-GCM authentication tag must match the pre-migration golden value"
        );
    }

    /// Golden regression test: decrypts the fixed PRE-migration ciphertext and
    /// tag captured by [`test_golden_encrypt_aes_gcm`] using the same fixed key
    /// and nonce, asserting the exact original plaintext. This locks the
    /// AES-256-GCM decrypt primitive used by [`EncryptionService::decrypt`]
    /// against fixed hardcoded inputs, so a byte-level change in the cipher
    /// output/behavior is caught (not merely a self-consistent round trip).
    #[test]
    fn test_golden_decrypt_aes_gcm() {
        use aes_gcm::{
            aead::{Aead, KeyInit},
            Aes256Gcm, Nonce,
        };
        let key = *b"0123456789abcdef0123456789abcdef";
        let nonce_bytes = *b"golden-nonce";
        let mut combined = hex::decode("6329becba284eaa9fa4ab0b2321a3b51ab4075b9e669")
            .expect("golden ciphertext hex must decode");
        let tag =
            hex::decode("9cc1f50490e6f90c1eb2b0927c18edb9").expect("golden tag hex must decode");
        combined.extend_from_slice(&tag);
        let cipher = Aes256Gcm::new(&key.into());
        let nonce = Nonce::from(nonce_bytes);
        let plaintext = cipher
            .decrypt(&nonce, combined.as_ref())
            .expect("aes-gcm decryption of fixed golden ciphertext must succeed");
        assert_eq!(
            plaintext, b"golden-plaintext-value",
            "decrypted plaintext must match the fixed golden input"
        );
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
