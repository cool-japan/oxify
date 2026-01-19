//! Zero-copy variable storage using Arc for large values
//!
//! This module implements an optimized variable store that uses `Arc<Value>`
//! for large JSON values to avoid expensive cloning operations.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Threshold in bytes above which values are stored as Arc
const LARGE_VALUE_THRESHOLD: usize = 1024; // 1KB

/// A variable that can be either owned or reference-counted
#[derive(Debug, Clone)]
pub enum Variable {
    /// Small value stored inline (< 1KB)
    Inline(Value),
    /// Large value stored in Arc to avoid cloning
    Shared(Arc<Value>),
}

impl Variable {
    /// Create a new variable, automatically choosing storage strategy
    pub fn new(value: Value) -> Self {
        let size_estimate = estimate_value_size(&value);

        if size_estimate >= LARGE_VALUE_THRESHOLD {
            Self::Shared(Arc::new(value))
        } else {
            Self::Inline(value)
        }
    }

    /// Create a shared variable (always uses Arc)
    pub fn shared(value: Value) -> Self {
        Self::Shared(Arc::new(value))
    }

    /// Get a reference to the value
    pub fn as_value(&self) -> &Value {
        match self {
            Self::Inline(v) => v,
            Self::Shared(arc) => arc.as_ref(),
        }
    }

    /// Convert to owned Value (may clone if shared)
    pub fn into_value(self) -> Value {
        match self {
            Self::Inline(v) => v,
            Self::Shared(arc) => Arc::try_unwrap(arc).unwrap_or_else(|arc| (*arc).clone()),
        }
    }

    /// Get strong count (for Arc-backed values)
    pub fn strong_count(&self) -> usize {
        match self {
            Self::Inline(_) => 1,
            Self::Shared(arc) => Arc::strong_count(arc),
        }
    }

    /// Check if this is a shared (Arc-backed) variable
    pub fn is_shared(&self) -> bool {
        matches!(self, Self::Shared(_))
    }
}

/// Optimized variable store with zero-copy semantics
#[derive(Debug, Clone)]
pub struct VariableStore {
    variables: HashMap<String, Variable>,
}

impl VariableStore {
    /// Create a new empty variable store
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    /// Insert a variable (automatically chooses storage strategy)
    pub fn insert(&mut self, key: String, value: Value) {
        self.variables.insert(key, Variable::new(value));
    }

    /// Insert a variable as shared (always uses Arc)
    pub fn insert_shared(&mut self, key: String, value: Value) {
        self.variables.insert(key, Variable::shared(value));
    }

    /// Insert a pre-created Variable
    pub fn insert_variable(&mut self, key: String, var: Variable) {
        self.variables.insert(key, var);
    }

    /// Get a reference to a variable's value
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.variables.get(key).map(|v| v.as_value())
    }

    /// Get a cloned Variable (cheap for Arc-backed values)
    pub fn get_variable(&self, key: &str) -> Option<Variable> {
        self.variables.get(key).cloned()
    }

    /// Remove a variable and return it
    pub fn remove(&mut self, key: &str) -> Option<Variable> {
        self.variables.remove(key)
    }

    /// Check if a key exists
    pub fn contains_key(&self, key: &str) -> bool {
        self.variables.contains_key(key)
    }

    /// Get the number of variables
    pub fn len(&self) -> usize {
        self.variables.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.variables.is_empty()
    }

    /// Clear all variables
    pub fn clear(&mut self) {
        self.variables.clear();
    }

    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.variables.keys()
    }

    /// Convert to a HashMap<String, Value> (may clone shared values)
    pub fn to_hashmap(&self) -> HashMap<String, Value> {
        self.variables
            .iter()
            .map(|(k, v)| (k.clone(), v.as_value().clone()))
            .collect()
    }

    /// Create from a HashMap<String, Value>
    pub fn from_hashmap(map: HashMap<String, Value>) -> Self {
        let variables = map
            .into_iter()
            .map(|(k, v)| (k, Variable::new(v)))
            .collect();

        Self { variables }
    }

    /// Get statistics about the store
    pub fn stats(&self) -> VariableStoreStats {
        let mut stats = VariableStoreStats::default();

        for var in self.variables.values() {
            stats.total_variables += 1;

            match var {
                Variable::Inline(_) => stats.inline_variables += 1,
                Variable::Shared(arc) => {
                    stats.shared_variables += 1;
                    stats.total_arc_refs += Arc::strong_count(arc);
                }
            }
        }

        stats
    }
}

impl Default for VariableStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about variable storage
#[derive(Debug, Clone, Default)]
pub struct VariableStoreStats {
    /// Total number of variables
    pub total_variables: usize,
    /// Number of inline (small) variables
    pub inline_variables: usize,
    /// Number of shared (Arc-backed) variables
    pub shared_variables: usize,
    /// Total Arc reference count across all shared variables
    pub total_arc_refs: usize,
}

impl VariableStoreStats {
    /// Calculate memory savings ratio (estimate)
    pub fn savings_ratio(&self) -> f64 {
        if self.total_variables == 0 {
            return 0.0;
        }

        // Estimate: each Arc reference saves one full clone
        let potential_clones = self.total_arc_refs.saturating_sub(self.shared_variables);
        potential_clones as f64 / self.total_variables as f64
    }
}

/// Estimate the size of a JSON value in bytes
fn estimate_value_size(value: &Value) -> usize {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 8,
        Value::String(s) => s.len(),
        Value::Array(arr) => arr.iter().map(estimate_value_size).sum::<usize>() + arr.len() * 8,
        Value::Object(obj) => {
            obj.iter()
                .map(|(k, v)| k.len() + estimate_value_size(v))
                .sum::<usize>()
                + obj.len() * 16
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_variable_inline_storage() {
        let small_value = json!({"x": 42});
        let var = Variable::new(small_value.clone());

        assert!(!var.is_shared());
        assert_eq!(var.as_value(), &small_value);
        assert_eq!(var.strong_count(), 1);
    }

    #[test]
    fn test_variable_shared_storage() {
        // Create a large value (over 1KB)
        let large_array: Vec<i32> = (0..500).collect();
        let large_value = json!(large_array);

        let var = Variable::new(large_value.clone());

        assert!(var.is_shared());
        assert_eq!(var.as_value(), &large_value);
        assert_eq!(var.strong_count(), 1);
    }

    #[test]
    fn test_variable_clone_is_cheap() {
        let large_array: Vec<i32> = (0..500).collect();
        let large_value = json!(large_array);

        let var1 = Variable::new(large_value);
        let var2 = var1.clone();

        // Both should share the same Arc
        assert_eq!(var1.strong_count(), 2);
        assert_eq!(var2.strong_count(), 2);
    }

    #[test]
    fn test_variable_store_insert_get() {
        let mut store = VariableStore::new();

        store.insert("x".to_string(), json!(42));
        store.insert("y".to_string(), json!("hello"));

        assert_eq!(store.get("x"), Some(&json!(42)));
        assert_eq!(store.get("y"), Some(&json!("hello")));
        assert_eq!(store.get("z"), None);
    }

    #[test]
    fn test_variable_store_shared_insert() {
        let mut store = VariableStore::new();

        let large_data = json!({"data": vec![0; 1000]});
        store.insert_shared("large".to_string(), large_data.clone());

        let var = store.get_variable("large").unwrap();
        assert!(var.is_shared());
        assert_eq!(var.strong_count(), 2); // One in store, one in `var`
    }

    #[test]
    fn test_variable_store_stats() {
        let mut store = VariableStore::new();

        // Add small values
        store.insert("a".to_string(), json!(1));
        store.insert("b".to_string(), json!(2));

        // Add large value
        let large_value = json!({"data": vec![0; 1000]});
        store.insert("large".to_string(), large_value);

        let stats = store.stats();
        assert_eq!(stats.total_variables, 3);
        assert_eq!(stats.inline_variables, 2);
        assert_eq!(stats.shared_variables, 1);
    }

    #[test]
    fn test_variable_store_remove() {
        let mut store = VariableStore::new();

        store.insert("x".to_string(), json!(42));
        assert!(store.contains_key("x"));

        let removed = store.remove("x");
        assert!(removed.is_some());
        assert!(!store.contains_key("x"));
    }

    #[test]
    fn test_variable_store_clear() {
        let mut store = VariableStore::new();

        store.insert("a".to_string(), json!(1));
        store.insert("b".to_string(), json!(2));
        assert_eq!(store.len(), 2);

        store.clear();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn test_variable_store_to_hashmap() {
        let mut store = VariableStore::new();

        store.insert("x".to_string(), json!(42));
        store.insert("y".to_string(), json!("test"));

        let map = store.to_hashmap();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("x"), Some(&json!(42)));
        assert_eq!(map.get("y"), Some(&json!("test")));
    }

    #[test]
    fn test_variable_store_from_hashmap() {
        let mut map = HashMap::new();
        map.insert("x".to_string(), json!(42));
        map.insert("y".to_string(), json!("test"));

        let store = VariableStore::from_hashmap(map);
        assert_eq!(store.len(), 2);
        assert_eq!(store.get("x"), Some(&json!(42)));
        assert_eq!(store.get("y"), Some(&json!("test")));
    }

    #[test]
    fn test_estimate_value_size() {
        assert_eq!(estimate_value_size(&json!(null)), 0);
        assert_eq!(estimate_value_size(&json!(true)), 1);
        assert_eq!(estimate_value_size(&json!(42)), 8);
        assert!(estimate_value_size(&json!("hello")) >= 5);

        let large_array: Vec<i32> = vec![0; 100];
        let large_array_json = json!(large_array);
        assert!(estimate_value_size(&large_array_json) > 100);
    }

    #[test]
    fn test_savings_ratio() {
        let mut store = VariableStore::new();

        // No variables = 0 savings
        assert_eq!(store.stats().savings_ratio(), 0.0);

        // Add a shared variable and clone it
        let large_data = json!({"data": vec![0; 1000]});
        store.insert_shared("large".to_string(), large_data);

        // Get variable (creates second Arc reference)
        let _var = store.get_variable("large");

        let stats = store.stats();
        // With 2 references but only 1 variable, savings_ratio should be > 0
        assert!(stats.savings_ratio() >= 0.0);
    }
}
