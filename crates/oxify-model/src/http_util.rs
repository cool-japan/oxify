//! Shared HTTP helper utilities for the OxiFY workspace.
//!
//! This module hosts small, dependency-light helpers used while migrating the
//! HTTP client stack from `reqwest` to the COOLJAPAN pure-Rust `oxihttp` crate.
//! `oxihttp` has no built-in query-parameter builder, so [`append_query_params`]
//! provides a drop-in replacement for reqwest's `.query(&[(k, v)])` semantics.

use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

/// Append percent-encoded query parameters to a URL, mirroring reqwest
/// .query(&[(k,v)]) semantics closely enough to be a drop-in replacement
/// for oxihttp, which has no built-in query-parameter API.
pub fn append_query_params<K, V>(url: &str, params: &[(K, V)]) -> String
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    if params.is_empty() {
        return url.to_string();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    let encoded: String = params
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                utf8_percent_encode(k.as_ref(), NON_ALPHANUMERIC),
                utf8_percent_encode(v.as_ref(), NON_ALPHANUMERIC)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{url}{sep}{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_params_returns_url_unchanged() {
        let params: &[(&str, &str)] = &[];
        assert_eq!(
            append_query_params("https://example.com/api", params),
            "https://example.com/api"
        );
    }

    #[test]
    fn single_param_appends_with_question_mark() {
        let params = &[("q", "rust")];
        assert_eq!(
            append_query_params("https://example.com/search", params),
            "https://example.com/search?q=rust"
        );
    }

    #[test]
    fn multiple_params_joined_with_ampersand() {
        let params = &[("q", "rust"), ("limit", "10")];
        assert_eq!(
            append_query_params("https://example.com/search", params),
            "https://example.com/search?q=rust&limit=10"
        );
    }

    #[test]
    fn existing_query_string_appends_with_ampersand() {
        let params = &[("page", "2")];
        assert_eq!(
            append_query_params("https://example.com/search?q=rust", params),
            "https://example.com/search?q=rust&page=2"
        );
    }

    #[test]
    fn special_characters_are_percent_encoded() {
        let params = &[("name", "a b&c=d"), ("k/v", "x?y")];
        assert_eq!(
            append_query_params("https://example.com/api", params),
            "https://example.com/api?name=a%20b%26c%3Dd&k%2Fv=x%3Fy"
        );
    }

    #[test]
    fn accepts_owned_string_keys_and_values() {
        let params = vec![(String::from("key"), String::from("value"))];
        assert_eq!(
            append_query_params("https://example.com", &params),
            "https://example.com?key=value"
        );
    }
}
