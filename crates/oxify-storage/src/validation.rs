//! Validation utilities for storage layer
//!
//! Provides reusable validation functions to ensure data consistency
//! and reduce code duplication across store modules.

use crate::{Result, StorageError};
use std::fmt::Display;

/// Validate that a value is positive
///
/// # Examples
/// ```
/// # use oxify_storage::validation::validate_positive;
/// assert!(validate_positive(10, "count").is_ok());
/// assert!(validate_positive(0, "count").is_err());
/// assert!(validate_positive(-5, "count").is_err());
/// ```
pub fn validate_positive<T: PartialOrd + Default + Display>(
    value: T,
    field_name: &str,
) -> Result<T> {
    if value <= T::default() {
        return Err(StorageError::validation(format!(
            "{} must be positive, got {}",
            field_name, value
        )));
    }
    Ok(value)
}

/// Validate optional positive value
///
/// Returns Ok(None) if value is None, validates if Some
pub fn validate_optional_positive<T: PartialOrd + Default + Display + Copy>(
    value: Option<T>,
    field_name: &str,
) -> Result<Option<T>> {
    if let Some(v) = value {
        validate_positive(v, field_name)?;
    }
    Ok(value)
}

/// Validate string is not empty or whitespace
///
/// # Examples
/// ```
/// # use oxify_storage::validation::validate_non_empty_string;
/// assert!(validate_non_empty_string("test", "field").is_ok());
/// assert!(validate_non_empty_string("", "field").is_err());
/// assert!(validate_non_empty_string("   ", "field").is_err());
/// ```
pub fn validate_non_empty_string(value: &str, field_name: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(StorageError::validation(format!(
            "{} cannot be empty",
            field_name
        )));
    }
    Ok(())
}

/// Validate collection size is within bounds
///
/// # Examples
/// ```
/// # use oxify_storage::validation::validate_collection_size;
/// let items = vec![1, 2, 3];
/// assert!(validate_collection_size(&items, 5, "items").is_ok());
/// assert!(validate_collection_size(&items, 2, "items").is_err());
/// ```
pub fn validate_collection_size<T>(
    collection: &[T],
    max_size: usize,
    collection_name: &str,
) -> Result<()> {
    if collection.len() > max_size {
        return Err(StorageError::validation(format!(
            "{} has {} items, which exceeds the maximum of {}",
            collection_name,
            collection.len(),
            max_size
        )));
    }
    Ok(())
}

/// Builder for quota limit validation
///
/// Accumulates validation errors and reports them all at once.
///
/// # Examples
/// ```
/// # use oxify_storage::validation::QuotaLimitValidator;
/// let result = QuotaLimitValidator::new()
///     .validate_limit(Some(100), "max_executions")
///     .validate_limit(Some(50), "max_tokens")
///     .build();
/// assert!(result.is_ok());
///
/// let result = QuotaLimitValidator::new()
///     .validate_limit(Some(-1), "max_executions")
///     .validate_limit(Some(0), "max_tokens")
///     .build();
/// assert!(result.is_err());
/// ```
pub struct QuotaLimitValidator {
    errors: Vec<String>,
}

impl QuotaLimitValidator {
    /// Create a new validator
    pub fn new() -> Self {
        Self { errors: Vec::new() }
    }

    /// Validate an optional i32 limit value
    pub fn validate_limit(mut self, value: Option<i32>, name: &str) -> Self {
        if let Some(limit) = value {
            if limit <= 0 {
                self.errors
                    .push(format!("{} must be positive, got {}", name, limit));
            }
        }
        self
    }

    /// Validate an optional i64 limit value
    pub fn validate_limit_i64(mut self, value: Option<i64>, name: &str) -> Self {
        if let Some(limit) = value {
            if limit <= 0 {
                self.errors
                    .push(format!("{} must be positive, got {}", name, limit));
            }
        }
        self
    }

    /// Build the validation result
    ///
    /// Returns Ok if no errors, or Err with all accumulated errors
    pub fn build(self) -> Result<()> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(StorageError::validation(self.errors.join("; ")))
        }
    }
}

impl Default for QuotaLimitValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_positive_i32() {
        assert!(validate_positive(10, "test").is_ok());
        assert!(validate_positive(1, "test").is_ok());

        let result = validate_positive(0, "test");
        assert!(result.is_err());

        let result = validate_positive(-5, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_positive_i64() {
        assert!(validate_positive(100i64, "test").is_ok());
        assert!(validate_positive(0i64, "test").is_err());
        assert!(validate_positive(-10i64, "test").is_err());
    }

    #[test]
    fn test_validate_optional_positive() {
        assert!(validate_optional_positive(Some(10), "test").is_ok());
        assert!(validate_optional_positive(None::<i32>, "test").is_ok());
        assert!(validate_optional_positive(Some(0), "test").is_err());
        assert!(validate_optional_positive(Some(-5), "test").is_err());
    }

    #[test]
    fn test_validate_non_empty_string() {
        assert!(validate_non_empty_string("test", "field").is_ok());
        assert!(validate_non_empty_string("a", "field").is_ok());

        let result = validate_non_empty_string("", "field");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));

        let result = validate_non_empty_string("   ", "field");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_collection_size() {
        let items = vec![1, 2, 3];
        assert!(validate_collection_size(&items, 5, "items").is_ok());
        assert!(validate_collection_size(&items, 3, "items").is_ok());

        let result = validate_collection_size(&items, 2, "items");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("exceeds the maximum"));
    }

    #[test]
    fn test_quota_limit_validator_valid() {
        let result = QuotaLimitValidator::new()
            .validate_limit(Some(100), "max_executions")
            .validate_limit(Some(50), "max_tokens")
            .validate_limit_i64(Some(1000), "max_cost")
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_quota_limit_validator_with_none() {
        let result = QuotaLimitValidator::new()
            .validate_limit(None, "max_executions")
            .validate_limit(Some(50), "max_tokens")
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_quota_limit_validator_invalid() {
        let result = QuotaLimitValidator::new()
            .validate_limit(Some(-1), "max_executions")
            .validate_limit(Some(0), "max_tokens")
            .build();

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("max_executions"));
        assert!(error_msg.contains("max_tokens"));
    }

    #[test]
    fn test_quota_limit_validator_multiple_errors() {
        let result = QuotaLimitValidator::new()
            .validate_limit(Some(-1), "field1")
            .validate_limit(Some(0), "field2")
            .validate_limit_i64(Some(-100), "field3")
            .build();

        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("field1"));
        assert!(error_msg.contains("field2"));
        assert!(error_msg.contains("field3"));
    }
}
