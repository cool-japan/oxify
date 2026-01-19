//! Schema versioning for model evolution
//!
//! This module provides versioning for the data models to enable
//! forward and backward compatibility.

use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Current schema version
pub const CURRENT_SCHEMA_VERSION: SchemaVersion = SchemaVersion {
    major: 1,
    minor: 1,
    patch: 0,
};

/// Schema version identifier
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct SchemaVersion {
    /// Major version - breaking changes
    pub major: u32,

    /// Minor version - new features, backward compatible
    pub minor: u32,

    /// Patch version - bug fixes
    pub patch: u32,
}

impl SchemaVersion {
    /// Create a new schema version
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Parse version string (e.g., "1.0.0")
    pub fn parse(version: &str) -> Result<Self, String> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(format!(
                "Invalid version format '{}': expected 'major.minor.patch'",
                version
            ));
        }

        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| format!("Invalid major version: {}", parts[0]))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|_| format!("Invalid minor version: {}", parts[1]))?;
        let patch = parts[2]
            .parse::<u32>()
            .map_err(|_| format!("Invalid patch version: {}", parts[2]))?;

        Ok(Self::new(major, minor, patch))
    }

    /// Check if this version is compatible with another
    /// (same major version, this >= other)
    pub fn is_compatible_with(&self, other: &SchemaVersion) -> bool {
        self.major == other.major
            && (self.minor > other.minor
                || (self.minor == other.minor && self.patch >= other.patch))
    }

    /// Check if this version is newer than another
    pub fn is_newer_than(&self, other: &SchemaVersion) -> bool {
        self.major > other.major
            || (self.major == other.major && self.minor > other.minor)
            || (self.major == other.major && self.minor == other.minor && self.patch > other.patch)
    }

    /// Check if this version requires migration from another
    pub fn requires_migration_from(&self, other: &SchemaVersion) -> bool {
        self.major != other.major
    }
}

impl Default for SchemaVersion {
    fn default() -> Self {
        CURRENT_SCHEMA_VERSION
    }
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for SchemaVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SchemaVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.major.cmp(&other.major) {
            std::cmp::Ordering::Equal => match self.minor.cmp(&other.minor) {
                std::cmp::Ordering::Equal => self.patch.cmp(&other.patch),
                other => other,
            },
            other => other,
        }
    }
}

/// Versioned container for serialized data
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Versioned<T> {
    /// Schema version of the data
    #[serde(default)]
    pub schema_version: SchemaVersion,

    /// The actual data
    pub data: T,

    /// Migration notes (if data was migrated)
    #[serde(default)]
    pub migration_notes: Vec<String>,
}

impl<T> Versioned<T> {
    /// Create a new versioned container with current schema version
    pub fn new(data: T) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            data,
            migration_notes: Vec::new(),
        }
    }

    /// Create with specific version
    pub fn with_version(data: T, version: SchemaVersion) -> Self {
        Self {
            schema_version: version,
            data,
            migration_notes: Vec::new(),
        }
    }

    /// Add migration note
    pub fn add_migration_note(&mut self, note: String) {
        self.migration_notes.push(note);
    }

    /// Check if data needs migration
    pub fn needs_migration(&self) -> bool {
        CURRENT_SCHEMA_VERSION.requires_migration_from(&self.schema_version)
    }
}

/// Schema migration trait
pub trait SchemaMigration<T> {
    /// Source version
    fn source_version(&self) -> SchemaVersion;

    /// Target version
    fn target_version(&self) -> SchemaVersion;

    /// Perform the migration
    fn migrate(&self, data: &mut T) -> Result<Vec<String>, String>;
}

/// Schema migration registry
pub struct MigrationRegistry<T> {
    migrations: Vec<Box<dyn SchemaMigration<T>>>,
}

impl<T> MigrationRegistry<T> {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Register a migration
    pub fn register(&mut self, migration: Box<dyn SchemaMigration<T>>) {
        self.migrations.push(migration);
    }

    /// Find migration path from source to target version
    pub fn find_migration_path(
        &self,
        from: &SchemaVersion,
        to: &SchemaVersion,
    ) -> Option<Vec<&dyn SchemaMigration<T>>> {
        if from >= to {
            return None;
        }

        let mut path = Vec::new();
        let mut current = *from;

        while current < *to {
            let next_migration = self
                .migrations
                .iter()
                .find(|m| m.source_version() == current && m.target_version() > current);

            match next_migration {
                Some(migration) => {
                    current = migration.target_version();
                    path.push(migration.as_ref());
                }
                None => return None,
            }
        }

        Some(path)
    }

    /// Apply all migrations to bring data up to current version
    pub fn migrate_to_current(&self, versioned: &mut Versioned<T>) -> Result<(), String> {
        if !versioned.needs_migration() {
            return Ok(());
        }

        let path = self
            .find_migration_path(&versioned.schema_version, &CURRENT_SCHEMA_VERSION)
            .ok_or_else(|| {
                format!(
                    "No migration path from {} to {}",
                    versioned.schema_version, CURRENT_SCHEMA_VERSION
                )
            })?;

        for migration in path {
            let notes = migration.migrate(&mut versioned.data)?;
            for note in notes {
                versioned.add_migration_note(note);
            }
            versioned.schema_version = migration.target_version();
        }

        Ok(())
    }
}

impl<T> Default for MigrationRegistry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Model metadata with schema version
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ModelMetadata {
    /// Schema version
    #[serde(default)]
    pub schema_version: SchemaVersion,

    /// Model type identifier (e.g., "workflow", "schedule", "template")
    pub model_type: String,

    /// Checksum for data integrity (optional)
    #[serde(default)]
    pub checksum: Option<String>,

    /// Serialization format used
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "json".to_string()
}

impl ModelMetadata {
    /// Create new metadata for a model type
    pub fn new(model_type: &str) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            model_type: model_type.to_string(),
            checksum: None,
            format: "json".to_string(),
        }
    }
}

/// Container for preserving unknown fields during deserialization
/// This enables forward compatibility - newer versions can be read by older code
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PreservedFields {
    /// Unknown fields stored as JSON values
    #[serde(flatten)]
    pub fields: std::collections::HashMap<String, serde_json::Value>,
}

impl PreservedFields {
    /// Create a new empty preserved fields container
    pub fn new() -> Self {
        Self {
            fields: std::collections::HashMap::new(),
        }
    }

    /// Add a preserved field
    pub fn add_field(&mut self, name: String, value: serde_json::Value) {
        self.fields.insert(name, value);
    }

    /// Get a preserved field
    pub fn get_field(&self, name: &str) -> Option<&serde_json::Value> {
        self.fields.get(name)
    }

    /// Check if there are any preserved fields
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Get the number of preserved fields
    pub fn len(&self) -> usize {
        self.fields.len()
    }
}

/// Enhanced versioned container with forward compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct VersionedWithCompat<T> {
    /// Schema version of the data
    #[serde(default)]
    pub schema_version: SchemaVersion,

    /// The actual data
    pub data: T,

    /// Migration notes (if data was migrated)
    #[serde(default)]
    pub migration_notes: Vec<String>,

    /// Preserved unknown fields for forward compatibility
    #[serde(default, flatten)]
    pub preserved: PreservedFields,
}

impl<T> VersionedWithCompat<T> {
    /// Create a new versioned container with current schema version
    pub fn new(data: T) -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            data,
            migration_notes: Vec::new(),
            preserved: PreservedFields::new(),
        }
    }

    /// Create with specific version
    pub fn with_version(data: T, version: SchemaVersion) -> Self {
        Self {
            schema_version: version,
            data,
            migration_notes: Vec::new(),
            preserved: PreservedFields::new(),
        }
    }

    /// Add migration note
    pub fn add_migration_note(&mut self, note: String) {
        self.migration_notes.push(note);
    }

    /// Check if data needs migration
    pub fn needs_migration(&self) -> bool {
        CURRENT_SCHEMA_VERSION.requires_migration_from(&self.schema_version)
    }

    /// Check if there are preserved fields from a newer version
    pub fn has_preserved_fields(&self) -> bool {
        !self.preserved.is_empty()
    }
}

/// Deprecated field mapping for backward compatibility
#[derive(Debug, Clone)]
pub struct DeprecatedField {
    /// Old field name
    pub old_name: String,

    /// New field name (replacement)
    pub new_name: String,

    /// Version when the field was deprecated
    pub deprecated_in: SchemaVersion,

    /// Version when the field will be removed (optional)
    pub removed_in: Option<SchemaVersion>,
}

impl DeprecatedField {
    /// Create a new deprecated field mapping
    pub fn new(old_name: String, new_name: String, deprecated_in: SchemaVersion) -> Self {
        Self {
            old_name,
            new_name,
            deprecated_in,
            removed_in: None,
        }
    }

    /// Set the version when the field will be removed
    pub fn with_removal(mut self, removed_in: SchemaVersion) -> Self {
        self.removed_in = Some(removed_in);
        self
    }

    /// Check if the field is still supported in a version
    pub fn is_supported_in(&self, version: &SchemaVersion) -> bool {
        match &self.removed_in {
            Some(removed) => version < removed,
            None => true,
        }
    }
}

/// Field migration trait for automatic field transformations
pub trait FieldMigration {
    /// Get the deprecated field mappings
    fn deprecated_fields(&self) -> Vec<DeprecatedField>;

    /// Apply field migrations to JSON value
    fn migrate_fields(&self, value: &mut serde_json::Value) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();

        if let serde_json::Value::Object(map) = value {
            for field in self.deprecated_fields() {
                if let Some(old_value) = map.remove(&field.old_name) {
                    map.insert(field.new_name.clone(), old_value);
                    notes.push(format!(
                        "Migrated field '{}' to '{}'",
                        field.old_name, field.new_name
                    ));
                }
            }
        }

        Ok(notes)
    }
}

/// Backward compatibility helper
pub struct BackwardCompatibility {
    /// Registered deprecated field mappings
    pub fields: Vec<DeprecatedField>,
}

impl BackwardCompatibility {
    /// Create a new backward compatibility helper
    pub fn new() -> Self {
        Self { fields: Vec::new() }
    }

    /// Register a deprecated field
    pub fn register_deprecated_field(&mut self, field: DeprecatedField) {
        self.fields.push(field);
    }

    /// Apply all field migrations to a JSON value
    pub fn migrate_json(&self, value: &mut serde_json::Value) -> Result<Vec<String>, String> {
        let mut notes = Vec::new();

        if let serde_json::Value::Object(map) = value {
            for field in &self.fields {
                if let Some(old_value) = map.remove(&field.old_name) {
                    map.insert(field.new_name.clone(), old_value);
                    notes.push(format!(
                        "Migrated deprecated field '{}' to '{}' (deprecated in {})",
                        field.old_name, field.new_name, field.deprecated_in
                    ));
                }
            }
        }

        Ok(notes)
    }
}

impl Default for BackwardCompatibility {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_version_parsing() {
        let version = SchemaVersion::parse("1.2.3").unwrap();
        assert_eq!(version.major, 1);
        assert_eq!(version.minor, 2);
        assert_eq!(version.patch, 3);
    }

    #[test]
    fn test_version_comparison() {
        let v1 = SchemaVersion::new(1, 0, 0);
        let v2 = SchemaVersion::new(1, 1, 0);
        let v3 = SchemaVersion::new(2, 0, 0);

        assert!(v2.is_newer_than(&v1));
        assert!(v3.is_newer_than(&v2));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn test_compatibility() {
        let v1 = SchemaVersion::new(1, 0, 0);
        let v2 = SchemaVersion::new(1, 1, 0);
        let v3 = SchemaVersion::new(2, 0, 0);

        // Same major version, newer minor/patch is compatible
        assert!(v2.is_compatible_with(&v1));

        // Different major version is not compatible
        assert!(!v3.is_compatible_with(&v1));
    }

    #[test]
    fn test_version_display() {
        let version = SchemaVersion::new(1, 2, 3);
        assert_eq!(version.to_string(), "1.2.3");
    }

    #[test]
    fn test_versioned_container() {
        let data = "test data".to_string();
        let versioned = Versioned::new(data.clone());

        assert_eq!(versioned.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(versioned.data, data);
        assert!(versioned.migration_notes.is_empty());
    }

    #[test]
    fn test_migration_requirement() {
        let old_version = SchemaVersion::new(0, 1, 0);
        let same_major = SchemaVersion::new(1, 0, 0);

        assert!(CURRENT_SCHEMA_VERSION.requires_migration_from(&old_version));
        assert!(!CURRENT_SCHEMA_VERSION.requires_migration_from(&same_major));
    }

    #[test]
    fn test_preserved_fields_creation() {
        let mut preserved = PreservedFields::new();
        assert!(preserved.is_empty());
        assert_eq!(preserved.len(), 0);

        preserved.add_field("unknown_field".to_string(), serde_json::json!("value"));
        assert!(!preserved.is_empty());
        assert_eq!(preserved.len(), 1);

        let value = preserved.get_field("unknown_field");
        assert!(value.is_some());
        assert_eq!(value.unwrap(), &serde_json::json!("value"));
    }

    #[test]
    fn test_versioned_with_compat() {
        let data = "test data".to_string();
        let versioned = VersionedWithCompat::new(data.clone());

        assert_eq!(versioned.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(versioned.data, data);
        assert!(versioned.migration_notes.is_empty());
        assert!(!versioned.has_preserved_fields());
    }

    #[test]
    fn test_deprecated_field() {
        let field = DeprecatedField::new(
            "old_field".to_string(),
            "new_field".to_string(),
            SchemaVersion::new(1, 0, 0),
        );

        assert_eq!(field.old_name, "old_field");
        assert_eq!(field.new_name, "new_field");
        assert!(field.is_supported_in(&SchemaVersion::new(1, 0, 0)));

        let field_with_removal = field.with_removal(SchemaVersion::new(2, 0, 0));
        assert!(!field_with_removal.is_supported_in(&SchemaVersion::new(2, 0, 0)));
        assert!(field_with_removal.is_supported_in(&SchemaVersion::new(1, 5, 0)));
    }

    #[test]
    fn test_backward_compatibility_migration() {
        let mut compat = BackwardCompatibility::new();

        let field = DeprecatedField::new(
            "oldName".to_string(),
            "new_name".to_string(),
            SchemaVersion::new(1, 0, 0),
        );
        compat.register_deprecated_field(field);

        let mut json = serde_json::json!({
            "oldName": "test_value",
            "other_field": 123
        });

        let notes = compat.migrate_json(&mut json).unwrap();
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("oldName"));
        assert!(notes[0].contains("new_name"));

        // Check that the old field was renamed
        assert!(json.get("oldName").is_none());
        assert_eq!(json.get("new_name").unwrap(), "test_value");
        assert_eq!(json.get("other_field").unwrap(), 123);
    }

    #[test]
    fn test_backward_compatibility_multiple_fields() {
        let mut compat = BackwardCompatibility::new();

        compat.register_deprecated_field(DeprecatedField::new(
            "field1".to_string(),
            "new_field1".to_string(),
            SchemaVersion::new(1, 0, 0),
        ));

        compat.register_deprecated_field(DeprecatedField::new(
            "field2".to_string(),
            "new_field2".to_string(),
            SchemaVersion::new(1, 0, 0),
        ));

        let mut json = serde_json::json!({
            "field1": "value1",
            "field2": "value2",
            "unchanged": "value3"
        });

        let notes = compat.migrate_json(&mut json).unwrap();
        assert_eq!(notes.len(), 2);

        assert!(json.get("field1").is_none());
        assert!(json.get("field2").is_none());
        assert_eq!(json.get("new_field1").unwrap(), "value1");
        assert_eq!(json.get("new_field2").unwrap(), "value2");
        assert_eq!(json.get("unchanged").unwrap(), "value3");
    }

    #[test]
    fn test_preserved_fields_serialization() {
        let mut preserved = PreservedFields::new();
        preserved.add_field("future_field".to_string(), serde_json::json!(42));
        preserved.add_field("another_field".to_string(), serde_json::json!("test"));

        let json = serde_json::to_string(&preserved).unwrap();
        let deserialized: PreservedFields = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized.get_field("future_field").unwrap(), 42);
        assert_eq!(deserialized.get_field("another_field").unwrap(), "test");
    }
}
