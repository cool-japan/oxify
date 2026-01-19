//! JWT token handling and validation
//!
//! Ported from `OxiRS` (<https://github.com/cool-japan/oxirs>)
//! Original implementation: Copyright (c) `OxiRS` Contributors
//! Adapted for `OxiFY`
//! License: MIT OR Apache-2.0 (compatible with `OxiRS`)
//!
//! # Features
//! - HS256/384/512 symmetric signing
//! - RS256/384/512 RSA asymmetric signing
//! - ES256/384 ECDSA asymmetric signing
//! - JWT ID (jti) claim for token revocation
//! - Refresh token support

use crate::types::{
    AuthError, Claims, JwtAlgorithm, JwtConfig, JwtKeyConfig, Result, TokenValidation, User,
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};

/// JWT token manager
pub struct JwtManager {
    encoding_key: Option<EncodingKey>,
    decoding_key: DecodingKey,
    algorithm: Algorithm,
    issuer: String,
    audience: String,
    expiration_secs: u64,
    refresh_expiration_secs: u64,
    include_jti: bool,
}

impl JwtManager {
    /// Create a new JWT manager
    pub fn new(config: &JwtConfig) -> Result<Self> {
        let algorithm = Self::convert_algorithm(config.algorithm);

        let (encoding_key, decoding_key) = Self::create_keys(config)?;

        Ok(Self {
            encoding_key,
            decoding_key,
            algorithm,
            issuer: config.issuer.clone(),
            audience: config.audience.clone(),
            expiration_secs: config.expiration_secs,
            refresh_expiration_secs: config.refresh_expiration_secs,
            include_jti: config.include_jti,
        })
    }

    /// Convert `JwtAlgorithm` to jsonwebtoken Algorithm
    fn convert_algorithm(alg: JwtAlgorithm) -> Algorithm {
        match alg {
            JwtAlgorithm::HS256 => Algorithm::HS256,
            JwtAlgorithm::HS384 => Algorithm::HS384,
            JwtAlgorithm::HS512 => Algorithm::HS512,
            JwtAlgorithm::RS256 => Algorithm::RS256,
            JwtAlgorithm::RS384 => Algorithm::RS384,
            JwtAlgorithm::RS512 => Algorithm::RS512,
            JwtAlgorithm::ES256 => Algorithm::ES256,
            JwtAlgorithm::ES384 => Algorithm::ES384,
        }
    }

    /// Create encoding and decoding keys from config
    fn create_keys(config: &JwtConfig) -> Result<(Option<EncodingKey>, DecodingKey)> {
        match &config.key_config {
            Some(JwtKeyConfig::Rsa {
                private_key,
                public_key,
            }) => {
                let encoding_key = if let Some(pk) = private_key {
                    Some(EncodingKey::from_rsa_pem(pk.as_bytes()).map_err(|e| {
                        AuthError::ConfigurationError(format!("Invalid RSA private key: {e}"))
                    })?)
                } else {
                    None
                };

                let decoding_key =
                    DecodingKey::from_rsa_pem(public_key.as_bytes()).map_err(|e| {
                        AuthError::ConfigurationError(format!("Invalid RSA public key: {e}"))
                    })?;

                Ok((encoding_key, decoding_key))
            }
            Some(JwtKeyConfig::Ec {
                private_key,
                public_key,
            }) => {
                let encoding_key = if let Some(pk) = private_key {
                    Some(EncodingKey::from_ec_pem(pk.as_bytes()).map_err(|e| {
                        AuthError::ConfigurationError(format!("Invalid EC private key: {e}"))
                    })?)
                } else {
                    None
                };

                let decoding_key =
                    DecodingKey::from_ec_pem(public_key.as_bytes()).map_err(|e| {
                        AuthError::ConfigurationError(format!("Invalid EC public key: {e}"))
                    })?;

                Ok((encoding_key, decoding_key))
            }
            Some(JwtKeyConfig::Secret(secret)) => {
                let encoding_key = EncodingKey::from_secret(secret.as_bytes());
                let decoding_key = DecodingKey::from_secret(secret.as_bytes());
                Ok((Some(encoding_key), decoding_key))
            }
            None => {
                // Use secret from config
                let encoding_key = EncodingKey::from_secret(config.secret.as_bytes());
                let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());
                Ok((Some(encoding_key), decoding_key))
            }
        }
    }

    /// Generate a JWT token for a user
    pub fn generate_token(&self, user: &User) -> Result<String> {
        let encoding_key = self.encoding_key.as_ref().ok_or_else(|| {
            AuthError::ConfigurationError(
                "No encoding key available (verification only mode)".into(),
            )
        })?;

        let now = Utc::now();
        let expiration = now + Duration::seconds(self.expiration_secs as i64);

        let jti = if self.include_jti {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        let claims = Claims {
            sub: user.username.clone(),
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            iss: self.issuer.clone(),
            aud: self.audience.clone(),
            roles: user.roles.clone(),
            permissions: user.permissions.clone(),
            jti,
            email: user.email.clone(),
            token_type: Some("access".to_string()),
        };

        encode(&Header::new(self.algorithm), &claims, encoding_key)
            .map_err(|e| AuthError::InternalError(format!("Failed to generate JWT token: {e}")))
    }

    /// Generate a token with custom claims
    pub fn generate_token_with_claims(&self, claims: &Claims) -> Result<String> {
        let encoding_key = self.encoding_key.as_ref().ok_or_else(|| {
            AuthError::ConfigurationError(
                "No encoding key available (verification only mode)".into(),
            )
        })?;

        encode(&Header::new(self.algorithm), claims, encoding_key)
            .map_err(|e| AuthError::InternalError(format!("Failed to generate JWT token: {e}")))
    }

    /// Validate a JWT token and return user information
    pub fn validate_token(&self, token: &str) -> Result<TokenValidation> {
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|_e| AuthError::InvalidToken("Failed to decode token".into()))?;

        let claims = token_data.claims;

        // Check if token is expired
        let exp_time = DateTime::from_timestamp(claims.exp, 0).ok_or(AuthError::InvalidToken(
            "Invalid expiration timestamp".into(),
        ))?;

        if Utc::now() > exp_time {
            return Err(AuthError::TokenExpired);
        }

        let user = User {
            username: claims.sub,
            roles: claims.roles,
            email: claims.email,
            full_name: None,
            last_login: None,
            permissions: claims.permissions,
        };

        Ok(TokenValidation {
            user,
            expires_at: exp_time,
        })
    }

    /// Validate token and return full claims
    pub fn validate_token_claims(&self, token: &str) -> Result<Claims> {
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|_e| AuthError::InvalidToken("Failed to decode token claims".into()))?;

        let claims = token_data.claims;

        // Check if token is expired
        let exp_time = DateTime::from_timestamp(claims.exp, 0).ok_or(AuthError::InvalidToken(
            "Invalid expiration timestamp in claims".into(),
        ))?;

        if Utc::now() > exp_time {
            return Err(AuthError::TokenExpired);
        }

        Ok(claims)
    }

    /// Get JWT ID (jti) from token without full validation
    pub fn get_token_id(&self, token: &str) -> Result<Option<String>> {
        // Use a permissive validation for extracting claims
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));
        validation.validate_exp = false; // Allow reading expired tokens

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|_e| AuthError::InvalidToken("Failed to extract token ID".into()))?;

        Ok(token_data.claims.jti)
    }

    /// Extract token from authorization header
    #[must_use]
    pub fn extract_token_from_header(auth_header: &str) -> Option<&str> {
        auth_header.strip_prefix("Bearer ")
    }

    /// Generate a refresh token
    pub fn generate_refresh_token(&self, user: &User) -> Result<String> {
        let encoding_key = self.encoding_key.as_ref().ok_or_else(|| {
            AuthError::ConfigurationError(
                "No encoding key available (verification only mode)".into(),
            )
        })?;

        let now = Utc::now();
        let expiration = now + Duration::seconds(self.refresh_expiration_secs as i64);

        let jti = if self.include_jti {
            Some(uuid::Uuid::new_v4().to_string())
        } else {
            None
        };

        let claims = Claims {
            sub: user.username.clone(),
            exp: expiration.timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            iss: self.issuer.clone(),
            aud: format!("{}-refresh", self.audience),
            roles: user.roles.clone(),
            permissions: user.permissions.clone(),
            jti,
            email: user.email.clone(),
            token_type: Some("refresh".to_string()),
        };

        encode(&Header::new(self.algorithm), &claims, encoding_key)
            .map_err(|e| AuthError::InternalError(format!("Failed to generate refresh token: {e}")))
    }

    /// Validate a refresh token
    pub fn validate_refresh_token(&self, token: &str) -> Result<TokenValidation> {
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        let audience = format!("{}-refresh", self.audience);
        validation.set_audience(std::slice::from_ref(&audience));

        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|_e| AuthError::InvalidToken("Failed to decode refresh token".into()))?;

        let claims = token_data.claims;

        let exp_time = DateTime::from_timestamp(claims.exp, 0).ok_or(AuthError::InvalidToken(
            "Invalid expiration timestamp in refresh token".into(),
        ))?;

        if Utc::now() > exp_time {
            return Err(AuthError::TokenExpired);
        }

        let user = User {
            username: claims.sub,
            roles: claims.roles,
            email: claims.email,
            full_name: None,
            last_login: None,
            permissions: claims.permissions,
        };

        Ok(TokenValidation {
            user,
            expires_at: exp_time,
        })
    }

    /// Get token expiration time
    pub fn get_token_expiration(&self, token: &str) -> Result<DateTime<Utc>> {
        // Use permissive validation to read expiration from potentially expired tokens
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(std::slice::from_ref(&self.issuer));
        validation.set_audience(std::slice::from_ref(&self.audience));
        validation.validate_exp = false; // Allow reading expired tokens

        let token_data =
            decode::<Claims>(token, &self.decoding_key, &validation).map_err(|_e| {
                AuthError::InvalidToken("Failed to decode token for expiration check".into())
            })?;

        DateTime::from_timestamp(token_data.claims.exp, 0).ok_or(AuthError::InvalidToken(
            "Invalid expiration timestamp for expiration check".into(),
        ))
    }

    /// Check if token is close to expiration (within 1 hour)
    pub fn is_token_close_to_expiration(&self, token: &str) -> Result<bool> {
        let expiration = self.get_token_expiration(token)?;
        let one_hour_from_now = Utc::now() + Duration::hours(1);
        Ok(expiration <= one_hour_from_now)
    }

    /// Get the issuer
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Get the audience
    #[must_use]
    pub fn audience(&self) -> &str {
        &self.audience
    }

    /// Check if manager can sign tokens
    #[must_use]
    pub fn can_sign(&self) -> bool {
        self.encoding_key.is_some()
    }
}

/// Builder for creating custom claims
pub struct ClaimsBuilder {
    sub: String,
    roles: Vec<String>,
    permissions: Vec<crate::types::Permission>,
    exp: i64,
    iat: i64,
    nbf: i64,
    iss: String,
    aud: String,
    jti: Option<String>,
    email: Option<String>,
    token_type: Option<String>,
}

impl ClaimsBuilder {
    /// Create a new claims builder
    pub fn new(
        subject: impl Into<String>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        let now = Utc::now().timestamp();
        Self {
            sub: subject.into(),
            roles: Vec::new(),
            permissions: Vec::new(),
            exp: now + 3600, // 1 hour default
            iat: now,
            nbf: now,
            iss: issuer.into(),
            aud: audience.into(),
            jti: None,
            email: None,
            token_type: None,
        }
    }

    /// Set roles
    #[must_use]
    pub fn roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }

    /// Set permissions
    #[must_use]
    pub fn permissions(mut self, permissions: Vec<crate::types::Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    /// Set expiration (duration from now)
    #[must_use]
    pub fn expires_in(mut self, duration: Duration) -> Self {
        self.exp = (Utc::now() + duration).timestamp();
        self
    }

    /// Set expiration (absolute timestamp)
    #[must_use]
    pub fn expires_at(mut self, time: DateTime<Utc>) -> Self {
        self.exp = time.timestamp();
        self
    }

    /// Set JWT ID
    #[must_use]
    pub fn jti(mut self, jti: impl Into<String>) -> Self {
        self.jti = Some(jti.into());
        self
    }

    /// Generate random JWT ID
    #[must_use]
    pub fn with_random_jti(mut self) -> Self {
        self.jti = Some(uuid::Uuid::new_v4().to_string());
        self
    }

    /// Set email
    #[must_use]
    pub fn email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Set token type
    #[must_use]
    pub fn token_type(mut self, token_type: impl Into<String>) -> Self {
        self.token_type = Some(token_type.into());
        self
    }

    /// Build the claims
    #[must_use]
    pub fn build(self) -> Claims {
        Claims {
            sub: self.sub,
            roles: self.roles,
            permissions: self.permissions,
            exp: self.exp,
            iat: self.iat,
            nbf: self.nbf,
            iss: self.iss,
            aud: self.aud,
            jti: self.jti,
            email: self.email,
            token_type: self.token_type,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Permission;

    #[test]
    fn test_jwt_token_generation() {
        let config = JwtConfig::development();
        let manager = JwtManager::new(&config).unwrap();

        let user = User {
            username: "alice".to_string(),
            roles: vec!["admin".to_string()],
            email: Some("alice@example.com".to_string()),
            full_name: Some("Alice Wonderland".to_string()),
            last_login: None,
            permissions: vec![Permission::Admin],
        };

        let token = manager.generate_token(&user).unwrap();
        assert!(!token.is_empty());

        // Validate the token
        let validation = manager.validate_token(&token).unwrap();
        assert_eq!(validation.user.username, "alice");
        assert_eq!(validation.user.roles.len(), 1);
    }

    #[test]
    fn test_jwt_with_jti() {
        let config = JwtConfig::development().with_jti(true);
        let manager = JwtManager::new(&config).unwrap();

        let user = User {
            username: "alice".to_string(),
            roles: vec!["admin".to_string()],
            email: None,
            full_name: None,
            last_login: None,
            permissions: vec![],
        };

        let token = manager.generate_token(&user).unwrap();

        // Get JTI
        let jti = manager.get_token_id(&token).unwrap();
        assert!(jti.is_some());

        // Validate full claims
        let claims = manager.validate_token_claims(&token).unwrap();
        assert!(claims.jti.is_some());
        assert_eq!(claims.token_type, Some("access".to_string()));
    }

    #[test]
    fn test_extract_token_from_header() {
        let header = "Bearer abc123";
        let token = JwtManager::extract_token_from_header(header);
        assert_eq!(token, Some("abc123"));

        let invalid_header = "Basic abc123";
        let token = JwtManager::extract_token_from_header(invalid_header);
        assert_eq!(token, None);
    }

    #[test]
    fn test_refresh_token() {
        let config = JwtConfig::development();
        let manager = JwtManager::new(&config).unwrap();

        let user = User {
            username: "bob".to_string(),
            roles: vec!["user".to_string()],
            email: None,
            full_name: None,
            last_login: None,
            permissions: vec![Permission::Read],
        };

        let refresh_token = manager.generate_refresh_token(&user).unwrap();
        assert!(!refresh_token.is_empty());

        // Validate the refresh token
        let validation = manager.validate_refresh_token(&refresh_token).unwrap();
        assert_eq!(validation.user.username, "bob");
    }

    #[test]
    fn test_invalid_token() {
        let config = JwtConfig::development();
        let manager = JwtManager::new(&config).unwrap();

        let invalid_token = "invalid.token.here";
        let result = manager.validate_token(invalid_token);
        assert!(result.is_err());
    }

    #[test]
    fn test_claims_builder() {
        let claims = ClaimsBuilder::new("user123", "issuer", "audience")
            .roles(vec!["admin".to_string()])
            .permissions(vec![Permission::Admin])
            .expires_in(Duration::hours(2))
            .with_random_jti()
            .email("user@example.com")
            .token_type("access")
            .build();

        assert_eq!(claims.sub, "user123");
        assert_eq!(claims.iss, "issuer");
        assert_eq!(claims.aud, "audience");
        assert!(claims.jti.is_some());
        assert_eq!(claims.email, Some("user@example.com".to_string()));
    }

    #[test]
    fn test_custom_claims_token() {
        let config = JwtConfig::development();
        let manager = JwtManager::new(&config).unwrap();

        let claims = ClaimsBuilder::new("user123", &config.issuer, &config.audience)
            .roles(vec!["custom".to_string()])
            .expires_in(Duration::minutes(30))
            .build();

        let token = manager.generate_token_with_claims(&claims).unwrap();
        assert!(!token.is_empty());

        let validated = manager.validate_token_claims(&token).unwrap();
        assert_eq!(validated.sub, "user123");
        assert_eq!(validated.roles, vec!["custom".to_string()]);
    }

    #[test]
    fn test_algorithm_selection() {
        // Test HS384
        let config = JwtConfig::new("secret", "issuer", "audience", 3600)
            .with_algorithm(JwtAlgorithm::HS384);
        let manager = JwtManager::new(&config).unwrap();
        assert!(manager.can_sign());

        // Test HS512
        let config = JwtConfig::new("secret", "issuer", "audience", 3600)
            .with_algorithm(JwtAlgorithm::HS512);
        let manager = JwtManager::new(&config).unwrap();

        let user = User {
            username: "test".to_string(),
            roles: vec![],
            email: None,
            full_name: None,
            last_login: None,
            permissions: vec![],
        };

        let token = manager.generate_token(&user).unwrap();
        let validation = manager.validate_token(&token).unwrap();
        assert_eq!(validation.user.username, "test");
    }
}
