//! # Property-Based Testing Helpers
//!
//! Generators and strategies for property-based testing using Proptest.
//! Tests invariants and edge cases automatically with random inputs.
//!
//! ## Example
//!
//! ```ignore
//! use oxify_authz::proptest_helpers::*;
//! use proptest::prelude::*;
//!
//! proptest! {
//!     #[test]
//!     fn test_relation_tuple_roundtrip(tuple in relation_tuple_strategy()) {
//!         // Property: serialization roundtrip should preserve data
//!         let json = serde_json::to_string(&tuple).unwrap();
//!         let deserialized: RelationTuple = serde_json::from_str(&json).unwrap();
//!         assert_eq!(tuple, deserialized);
//!     }
//! }
//! ```

use crate::{RelationTuple, Subject};
use proptest::prelude::*;

/// Strategy for generating valid namespace names
pub fn namespace_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-z][a-z0-9_]{2,20}").expect("invariant: valid regex literal")
}

/// Strategy for generating valid object IDs
pub fn object_id_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9_-]{1,50}").expect("invariant: valid regex literal")
}

/// Strategy for generating valid relation names
pub fn relation_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "owner", "editor", "viewer", "admin", "member", "parent", "reader", "writer",
        ]),
        1..=1,
    )
    .prop_map(|v| v[0].to_string())
}

/// Strategy for generating user IDs
pub fn user_id_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("user:[a-z0-9_]{3,20}").expect("invariant: valid regex literal")
}

/// Strategy for generating Subject variants
pub fn subject_strategy() -> impl Strategy<Value = Subject> {
    prop_oneof![
        user_id_strategy().prop_map(Subject::User),
        (
            namespace_strategy(),
            object_id_strategy(),
            relation_strategy()
        )
            .prop_map(|(namespace, object_id, relation)| Subject::UserSet {
                namespace,
                object_id,
                relation,
            }),
    ]
}

/// Strategy for generating valid RelationTuples
pub fn relation_tuple_strategy() -> impl Strategy<Value = RelationTuple> {
    (
        namespace_strategy(),
        relation_strategy(),
        object_id_strategy(),
        subject_strategy(),
    )
        .prop_map(|(namespace, relation, object_id, subject)| {
            RelationTuple::new(namespace, relation, object_id, subject)
        })
}

/// Strategy for generating a list of related tuples (same namespace)
pub fn related_tuples_strategy() -> impl Strategy<Value = Vec<RelationTuple>> {
    (
        namespace_strategy(),
        prop::collection::vec(
            (
                relation_strategy(),
                object_id_strategy(),
                subject_strategy(),
            ),
            1..10,
        ),
    )
        .prop_map(|(namespace, tuples)| {
            tuples
                .into_iter()
                .map(|(relation, object_id, subject)| {
                    RelationTuple::new(namespace.clone(), relation, object_id, subject)
                })
                .collect()
        })
}

/// Strategy for generating hierarchical permission tuples
/// Creates tuples with owner -> editor -> viewer hierarchy
pub fn hierarchical_tuples_strategy() -> impl Strategy<Value = Vec<RelationTuple>> {
    (
        namespace_strategy(),
        object_id_strategy(),
        user_id_strategy(),
    )
        .prop_map(|(namespace, object_id, user_id)| {
            vec![
                RelationTuple::new(
                    namespace.clone(),
                    "owner",
                    object_id.clone(),
                    Subject::User(user_id.clone()),
                ),
                RelationTuple::new(
                    namespace.clone(),
                    "editor",
                    object_id.clone(),
                    Subject::UserSet {
                        namespace: namespace.clone(),
                        object_id: object_id.clone(),
                        relation: "owner".to_string(),
                    },
                ),
                RelationTuple::new(
                    namespace.clone(),
                    "viewer",
                    object_id.clone(),
                    Subject::UserSet {
                        namespace: namespace.clone(),
                        object_id: object_id.clone(),
                        relation: "editor".to_string(),
                    },
                ),
            ]
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn test_namespace_generation(namespace in namespace_strategy()) {
            // Property: namespace should be lowercase and valid
            assert!(!namespace.is_empty());
            assert!(namespace.chars().all(|c| c.is_lowercase() || c.is_numeric() || c == '_'));
        }

        #[test]
        fn test_relation_tuple_generation(tuple in relation_tuple_strategy()) {
            // Property: all fields should be non-empty
            assert!(!tuple.namespace.is_empty());
            assert!(!tuple.relation.is_empty());
            assert!(!tuple.object_id.is_empty());
        }

        #[test]
        fn test_subject_generation(subject in subject_strategy()) {
            // Property: Subject should be valid
            match subject {
                Subject::User(id) => assert!(!id.is_empty()),
                Subject::UserSet { namespace, object_id, relation } => {
                    assert!(!namespace.is_empty());
                    assert!(!object_id.is_empty());
                    assert!(!relation.is_empty());
                }
            }
        }

        #[test]
        fn test_tuple_serialization_roundtrip(tuple in relation_tuple_strategy()) {
            // Property: JSON serialization should be reversible
            let json = serde_json::to_string(&tuple).unwrap();
            let deserialized: RelationTuple = serde_json::from_str(&json).unwrap();
            assert_eq!(tuple, deserialized);
        }

        #[test]
        fn test_related_tuples_same_namespace(tuples in related_tuples_strategy()) {
            // Property: All tuples should share the same namespace
            if !tuples.is_empty() {
                let first_namespace = &tuples[0].namespace;
                assert!(tuples.iter().all(|t| &t.namespace == first_namespace));
            }
        }

        #[test]
        fn test_hierarchical_tuples_structure(tuples in hierarchical_tuples_strategy()) {
            // Property: Should have exactly 3 tuples (owner, editor, viewer)
            assert_eq!(tuples.len(), 3);

            // Property: Relations should be in hierarchical order
            assert_eq!(tuples[0].relation, "owner");
            assert_eq!(tuples[1].relation, "editor");
            assert_eq!(tuples[2].relation, "viewer");

            // Property: All tuples should reference the same object
            let object_id = &tuples[0].object_id;
            assert!(tuples.iter().all(|t| &t.object_id == object_id));
        }
    }
}
