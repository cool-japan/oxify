//! Database utility functions
//!
//! Common database operations and type conversions used across store modules.

use uuid::Uuid;

/// Parse a vector of UUID strings to UUIDs, filtering out invalid ones
///
/// This is useful when retrieving UUID arrays from PostgreSQL text[] columns.
///
/// # Examples
/// ```
/// # use oxify_storage::db_utils::parse_uuid_array;
/// # use uuid::Uuid;
/// let valid_uuid = Uuid::new_v4().to_string();
/// let input = vec![
///     valid_uuid.clone(),
///     "invalid-uuid".to_string(),
///     Uuid::new_v4().to_string(),
/// ];
/// let result = parse_uuid_array(input);
/// assert_eq!(result.len(), 2); // Only valid UUIDs
/// ```
pub fn parse_uuid_array(strings: Vec<String>) -> Vec<Uuid> {
    strings
        .iter()
        .filter_map(|s| Uuid::parse_str(s).ok())
        .collect()
}

/// Convert `Vec<Uuid>` to `Vec<String>` for database storage
///
/// # Examples
/// ```
/// # use oxify_storage::db_utils::uuids_to_strings;
/// # use uuid::Uuid;
/// let uuids = vec![Uuid::new_v4(), Uuid::new_v4()];
/// let strings = uuids_to_strings(&uuids);
/// assert_eq!(strings.len(), 2);
/// ```
pub fn uuids_to_strings(uuids: &[Uuid]) -> Vec<String> {
    uuids.iter().map(|id| id.to_string()).collect()
}

/// Safe conversion from `i64` to `u64` with validation
///
/// Negative values are clamped to 0.
///
/// # Examples
/// ```
/// # use oxify_storage::db_utils::i64_to_u64_safe;
/// assert_eq!(i64_to_u64_safe(100), 100);
/// assert_eq!(i64_to_u64_safe(0), 0);
/// assert_eq!(i64_to_u64_safe(-1), 0);
/// assert_eq!(i64_to_u64_safe(-999), 0);
/// ```
#[inline]
pub fn i64_to_u64_safe(value: i64) -> u64 {
    value.max(0) as u64
}

/// Safe conversion from `Option<i64>` to `Option<u64>`
///
/// Negative values are clamped to 0.
///
/// # Examples
/// ```
/// # use oxify_storage::db_utils::opt_i64_to_u64;
/// assert_eq!(opt_i64_to_u64(Some(100)), Some(100));
/// assert_eq!(opt_i64_to_u64(Some(-1)), Some(0));
/// assert_eq!(opt_i64_to_u64(None), None);
/// ```
#[inline]
pub fn opt_i64_to_u64(value: Option<i64>) -> Option<u64> {
    value.map(|v| v.max(0) as u64)
}

/// Convert boolean to integer for database storage (PostgreSQL compatible)
///
/// # Examples
/// ```
/// # use oxify_storage::db_utils::bool_to_int;
/// assert_eq!(bool_to_int(true), 1);
/// assert_eq!(bool_to_int(false), 0);
/// ```
#[inline]
pub fn bool_to_int(value: bool) -> i32 {
    if value {
        1
    } else {
        0
    }
}

/// Convert integer to boolean from database
///
/// # Examples
/// ```
/// # use oxify_storage::db_utils::int_to_bool;
/// assert_eq!(int_to_bool(1), true);
/// assert_eq!(int_to_bool(0), false);
/// assert_eq!(int_to_bool(42), true); // Any non-zero is true
/// ```
#[inline]
pub fn int_to_bool(value: i32) -> bool {
    value != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_uuid_array() {
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        let input = vec![
            uuid1.to_string(),
            "invalid-uuid".to_string(),
            uuid2.to_string(),
            "not-a-uuid-at-all".to_string(),
        ];

        let result = parse_uuid_array(input);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&uuid1));
        assert!(result.contains(&uuid2));
    }

    #[test]
    fn test_parse_uuid_array_all_invalid() {
        let input = vec!["invalid-uuid".to_string(), "not-a-uuid".to_string()];

        let result = parse_uuid_array(input);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_parse_uuid_array_empty() {
        let input: Vec<String> = vec![];
        let result = parse_uuid_array(input);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_uuids_to_strings() {
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();
        let uuids = vec![uuid1, uuid2];

        let strings = uuids_to_strings(&uuids);
        assert_eq!(strings.len(), 2);
        assert_eq!(strings[0], uuid1.to_string());
        assert_eq!(strings[1], uuid2.to_string());
    }

    #[test]
    fn test_uuids_to_strings_empty() {
        let uuids: Vec<Uuid> = vec![];
        let strings = uuids_to_strings(&uuids);
        assert_eq!(strings.len(), 0);
    }

    #[test]
    fn test_i64_to_u64_safe() {
        assert_eq!(i64_to_u64_safe(100), 100);
        assert_eq!(i64_to_u64_safe(0), 0);
        assert_eq!(i64_to_u64_safe(-1), 0);
        assert_eq!(i64_to_u64_safe(-999), 0);
        assert_eq!(i64_to_u64_safe(i64::MAX), i64::MAX as u64);
    }

    #[test]
    fn test_opt_i64_to_u64() {
        assert_eq!(opt_i64_to_u64(Some(100)), Some(100));
        assert_eq!(opt_i64_to_u64(Some(0)), Some(0));
        assert_eq!(opt_i64_to_u64(Some(-1)), Some(0));
        assert_eq!(opt_i64_to_u64(Some(-999)), Some(0));
        assert_eq!(opt_i64_to_u64(None), None);
    }

    #[test]
    fn test_bool_to_int() {
        assert_eq!(bool_to_int(true), 1);
        assert_eq!(bool_to_int(false), 0);
    }

    #[test]
    fn test_int_to_bool() {
        assert!(int_to_bool(1));
        assert!(!int_to_bool(0));
        assert!(int_to_bool(42));
        assert!(int_to_bool(-1));
    }

    #[test]
    fn test_roundtrip_bool_int() {
        assert!(int_to_bool(bool_to_int(true)));
        assert!(!int_to_bool(bool_to_int(false)));
    }
}
