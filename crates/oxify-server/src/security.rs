//! Security headers middleware
//!
//! Adds security-related HTTP headers to prevent common attacks.

use axum::{
    extract::Request,
    http::header::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

/// Security headers configuration
#[derive(Debug, Clone)]
pub struct SecurityHeadersConfig {
    /// Content Security Policy
    pub csp: Option<String>,
    /// Enable X-Frame-Options
    pub x_frame_options: bool,
    /// Enable X-Content-Type-Options
    pub x_content_type_options: bool,
    /// Enable X-XSS-Protection
    pub x_xss_protection: bool,
    /// Enable Strict-Transport-Security (HSTS)
    pub hsts: Option<HstsConfig>,
    /// Referrer Policy
    pub referrer_policy: Option<String>,
    /// Permissions Policy
    pub permissions_policy: Option<String>,
}

impl Default for SecurityHeadersConfig {
    fn default() -> Self {
        Self {
            csp: Some("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'".to_string()),
            x_frame_options: true,
            x_content_type_options: true,
            x_xss_protection: true,
            hsts: Some(HstsConfig::default()),
            referrer_policy: Some("strict-origin-when-cross-origin".to_string()),
            permissions_policy: Some("geolocation=(), microphone=(), camera=()".to_string()),
        }
    }
}

/// HSTS (HTTP Strict Transport Security) configuration
#[derive(Debug, Clone)]
pub struct HstsConfig {
    /// Max age in seconds
    pub max_age: u32,
    /// Include subdomains
    pub include_subdomains: bool,
    /// Preload flag
    pub preload: bool,
}

impl Default for HstsConfig {
    fn default() -> Self {
        Self {
            max_age: 31536000, // 1 year
            include_subdomains: true,
            preload: false,
        }
    }
}

impl HstsConfig {
    /// Convert to header value string
    pub fn to_header_value(&self) -> String {
        let mut value = format!("max-age={}", self.max_age);
        if self.include_subdomains {
            value.push_str("; includeSubDomains");
        }
        if self.preload {
            value.push_str("; preload");
        }
        value
    }
}

/// Security headers middleware
///
/// Adds security headers to all responses to prevent common attacks.
pub async fn security_headers_middleware(
    config: SecurityHeadersConfig,
    request: Request,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // Content-Security-Policy
    if let Some(csp) = &config.csp {
        if let Ok(value) = HeaderValue::from_str(csp) {
            headers.insert(HeaderName::from_static("content-security-policy"), value);
        }
    }

    // X-Frame-Options (prevent clickjacking)
    if config.x_frame_options {
        headers.insert(
            HeaderName::from_static("x-frame-options"),
            HeaderValue::from_static("DENY"),
        );
    }

    // X-Content-Type-Options (prevent MIME sniffing)
    if config.x_content_type_options {
        headers.insert(
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        );
    }

    // X-XSS-Protection (legacy XSS protection)
    if config.x_xss_protection {
        headers.insert(
            HeaderName::from_static("x-xss-protection"),
            HeaderValue::from_static("1; mode=block"),
        );
    }

    // Strict-Transport-Security (HSTS)
    if let Some(hsts) = &config.hsts {
        if let Ok(value) = HeaderValue::from_str(&hsts.to_header_value()) {
            headers.insert(HeaderName::from_static("strict-transport-security"), value);
        }
    }

    // Referrer-Policy
    if let Some(referrer) = &config.referrer_policy {
        if let Ok(value) = HeaderValue::from_str(referrer) {
            headers.insert(HeaderName::from_static("referrer-policy"), value);
        }
    }

    // Permissions-Policy
    if let Some(permissions) = &config.permissions_policy {
        if let Ok(value) = HeaderValue::from_str(permissions) {
            headers.insert(HeaderName::from_static("permissions-policy"), value);
        }
    }

    response
}

/// Create a strict security headers configuration for production
pub fn strict_security_config() -> SecurityHeadersConfig {
    SecurityHeadersConfig {
        csp: Some("default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'".to_string()),
        x_frame_options: true,
        x_content_type_options: true,
        x_xss_protection: true,
        hsts: Some(HstsConfig {
            max_age: 63072000, // 2 years
            include_subdomains: true,
            preload: true,
        }),
        referrer_policy: Some("no-referrer".to_string()),
        permissions_policy: Some("geolocation=(), microphone=(), camera=(), payment=()".to_string()),
    }
}

/// Create a relaxed security headers configuration for development
pub fn relaxed_security_config() -> SecurityHeadersConfig {
    SecurityHeadersConfig {
        csp: None, // Disabled for development
        x_frame_options: false,
        x_content_type_options: true,
        x_xss_protection: true,
        hsts: None, // Disabled for development (HTTP)
        referrer_policy: Some("strict-origin-when-cross-origin".to_string()),
        permissions_policy: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hsts_config_default() {
        let hsts = HstsConfig::default();
        assert_eq!(hsts.max_age, 31536000);
        assert!(hsts.include_subdomains);
        assert!(!hsts.preload);
    }

    #[test]
    fn test_hsts_config_to_header_value() {
        let hsts = HstsConfig {
            max_age: 31536000,
            include_subdomains: true,
            preload: true,
        };
        let value = hsts.to_header_value();
        assert_eq!(value, "max-age=31536000; includeSubDomains; preload");
    }

    #[test]
    fn test_hsts_config_no_subdomains() {
        let hsts = HstsConfig {
            max_age: 3600,
            include_subdomains: false,
            preload: false,
        };
        let value = hsts.to_header_value();
        assert_eq!(value, "max-age=3600");
    }

    #[test]
    fn test_security_headers_config_default() {
        let config = SecurityHeadersConfig::default();
        assert!(config.csp.is_some());
        assert!(config.x_frame_options);
        assert!(config.x_content_type_options);
        assert!(config.x_xss_protection);
        assert!(config.hsts.is_some());
    }

    #[test]
    fn test_strict_security_config() {
        let config = strict_security_config();
        assert!(config.csp.is_some());
        assert!(config.hsts.is_some());
        if let Some(hsts) = config.hsts {
            assert_eq!(hsts.max_age, 63072000);
            assert!(hsts.preload);
        }
    }

    #[test]
    fn test_relaxed_security_config() {
        let config = relaxed_security_config();
        assert!(config.csp.is_none());
        assert!(!config.x_frame_options);
        assert!(config.hsts.is_none());
    }

    #[test]
    fn test_csp_format() {
        let config = SecurityHeadersConfig::default();
        if let Some(csp) = config.csp {
            assert!(csp.contains("default-src"));
            assert!(csp.contains("'self'"));
        }
    }
}
