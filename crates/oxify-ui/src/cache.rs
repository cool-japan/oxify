//! Response caching utilities and middleware

use axum::{
    http::{header, HeaderValue, Request, Response},
    middleware::Next,
};

/// Add cache-control headers for static assets
pub async fn cache_static_assets(
    req: Request<axum::body::Body>,
    next: Next,
) -> Response<axum::body::Body> {
    let mut response = next.run(req).await;

    // Add cache headers for static assets (1 year for immutable assets)
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    response
}

/// Add cache-control headers for API responses (short-term caching)
pub async fn cache_api_responses(
    req: Request<axum::body::Body>,
    next: Next,
) -> Response<axum::body::Body> {
    let method = req.method().clone();
    let mut response = next.run(req).await;

    // Only cache successful GET requests
    if method == axum::http::Method::GET && response.status().is_success() {
        // Cache for 5 minutes
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=300"),
        );
    } else {
        // No cache for mutations or errors
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate"),
        );
    }

    response
}

/// Add ETag support for conditional requests
pub async fn add_etag(req: Request<axum::body::Body>, next: Next) -> Response<axum::body::Body> {
    let response = next.run(req).await;

    // In a real implementation, we would:
    // 1. Generate ETag from response body hash
    // 2. Check If-None-Match header from request
    // 3. Return 304 Not Modified if ETags match
    //
    // For now, just pass through
    response
}

/// Cache configuration for different resource types
#[derive(Debug, Clone, Copy)]
pub enum CachePolicy {
    /// No caching
    NoCache,
    /// Short-term cache (5 minutes)
    ShortTerm,
    /// Medium-term cache (1 hour)
    MediumTerm,
    /// Long-term cache (1 day)
    LongTerm,
    /// Immutable cache (1 year)
    Immutable,
}

impl CachePolicy {
    pub fn to_header_value(self) -> HeaderValue {
        match self {
            CachePolicy::NoCache => HeaderValue::from_static("no-store, no-cache, must-revalidate"),
            CachePolicy::ShortTerm => HeaderValue::from_static("public, max-age=300"),
            CachePolicy::MediumTerm => HeaderValue::from_static("public, max-age=3600"),
            CachePolicy::LongTerm => HeaderValue::from_static("public, max-age=86400"),
            CachePolicy::Immutable => {
                HeaderValue::from_static("public, max-age=31536000, immutable")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_policy_headers() {
        assert_eq!(
            CachePolicy::NoCache.to_header_value(),
            HeaderValue::from_static("no-store, no-cache, must-revalidate")
        );
        assert_eq!(
            CachePolicy::ShortTerm.to_header_value(),
            HeaderValue::from_static("public, max-age=300")
        );
        assert_eq!(
            CachePolicy::Immutable.to_header_value(),
            HeaderValue::from_static("public, max-age=31536000, immutable")
        );
    }

    #[test]
    fn test_cache_policy_types() {
        // Test that all cache policies are defined
        let _no_cache = CachePolicy::NoCache;
        let _short = CachePolicy::ShortTerm;
        let _medium = CachePolicy::MediumTerm;
        let _long = CachePolicy::LongTerm;
        let _immutable = CachePolicy::Immutable;
    }
}
