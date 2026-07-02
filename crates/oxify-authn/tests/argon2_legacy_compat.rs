//! Mandatory legacy Argon2id compatibility gate.
//!
//! # Why this test exists
//!
//! Production `crates/oxify-authn/src/password.rs` currently hashes passwords
//! with the `argon2` crate (v0.5.3, resolved via `Cargo.lock`) using
//! `Argon2::default()` (Argon2id, v=19, m=19456, t=2, p=1). We intend to
//! migrate password verification to the COOLJAPAN pure-Rust KDF,
//! `oxicrypto_kdf::argon2id_verify_phc`.
//!
//! Every existing user's stored credential is a PHC string produced by the
//! legacy `argon2` 0.5.3 crate. If `oxicrypto_kdf::argon2id_verify_phc` cannot
//! verify those PHC strings, migrating `password.rs` would silently lock out
//! EVERY existing user on their next login. This test proves the new verifier
//! accepts a genuine legacy PHC string before any migration is allowed.
//!
//! # If this test ever fails
//!
//! STOP. Do NOT deploy a migration that swaps the verifier. A failure here
//! means the two implementations are not wire-compatible for stored hashes and
//! a lazy-rehash-on-login fallback strategy is required instead.
//!
//! # Reproducibility of `LEGACY_PHC`
//!
//! The `LEGACY_PHC` constant below was produced by the real `argon2` 0.5.3
//! crate (the exact version pinned in this workspace's `Cargo.lock`) with:
//!
//! ```ignore
//! use argon2::{password_hash::{PasswordHasher, SaltString}, Argon2};
//! let salt = SaltString::encode_b64(b"somesaltsomesalt").unwrap(); // fixed salt
//! let phc = Argon2::default()
//!     .hash_password(b"CorrectHorseBatteryStaple42!", &salt)
//!     .unwrap()
//!     .to_string();
//! // phc == LEGACY_PHC
//! ```
//!
//! - argon2 crate version: 0.5.3
//! - parameters: `Argon2::default()` == Argon2id, v=19, m=19456, t=2, p=1
//! - fixed salt (raw bytes): b"somesaltsomesalt" -> b64 "c29tZXNhbHRzb21lc2FsdA"
//! - password: "CorrectHorseBatteryStaple42!"
//!
//! Because the salt is fixed (not random), the value is fully reproducible and
//! auditable: re-running the snippet above with argon2 0.5.3 yields exactly
//! `LEGACY_PHC`.

use oxicrypto_core::CryptoError;
use oxicrypto_kdf::argon2id_verify_phc;

/// Fixed password used to generate `LEGACY_PHC` (see module docs).
const LEGACY_PASSWORD: &str = "CorrectHorseBatteryStaple42!";

/// Real PHC string emitted by argon2 0.5.3 (see module docs for full recipe).
const LEGACY_PHC: &str =
    "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHRzb21lc2FsdA$EqDnARqxvjvOd5J5IAka5y4ud640w8Y266utCYWh8AQ";

/// Gate 1: the new verifier MUST accept a genuine legacy argon2 0.5.3 PHC.
#[test]
fn legacy_phc_verifies_with_correct_password() {
    let result = argon2id_verify_phc(LEGACY_PHC, LEGACY_PASSWORD.as_bytes());
    assert_eq!(
        result,
        Ok(()),
        "oxicrypto_kdf::argon2id_verify_phc rejected a genuine argon2 0.5.3 PHC string; \
         migrating password.rs would lock out existing users. PHC={LEGACY_PHC}"
    );
}

/// Gate 2: a wrong password MUST be rejected with `CryptoError::InvalidTag`.
#[test]
fn legacy_phc_rejects_wrong_password() {
    let result = argon2id_verify_phc(LEGACY_PHC, b"totally-the-wrong-password");
    assert_eq!(
        result,
        Err(CryptoError::InvalidTag),
        "wrong password should return CryptoError::InvalidTag, got {result:?}"
    );
}

/// Gate 3: a malformed (non-PHC) string MUST be rejected with `CryptoError::Encoding`.
#[test]
fn malformed_phc_returns_encoding_error() {
    let result = argon2id_verify_phc("this-is-not-a-valid-phc-string", LEGACY_PASSWORD.as_bytes());
    assert_eq!(
        result,
        Err(CryptoError::Encoding),
        "malformed PHC string should return CryptoError::Encoding, got {result:?}"
    );
}
