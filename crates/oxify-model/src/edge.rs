use crate::NodeId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// Unique identifier for an edge
pub type EdgeId = Uuid;

/// Edge connecting nodes in the workflow DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Edge {
    /// Unique edge identifier
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub id: EdgeId,

    /// Source node
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub from: NodeId,

    /// Target node
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub to: NodeId,

    /// Optional label for the edge
    pub label: Option<String>,

    /// Condition for this edge (for conditional nodes)
    pub condition: Option<String>,
}

impl Edge {
    pub fn new(from: NodeId, to: NodeId) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            label: None,
            condition: None,
        }
    }

    pub fn with_label(mut self, label: String) -> Self {
        self.label = Some(label);
        self
    }

    pub fn with_condition(mut self, condition: String) -> Self {
        self.condition = Some(condition);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_creation() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = Edge::new(from, to)
            .with_label("success".to_string())
            .with_condition("result.ok".to_string());

        assert_eq!(edge.from, from);
        assert_eq!(edge.to, to);
        assert_eq!(edge.label, Some("success".to_string()));
        assert_eq!(edge.condition, Some("result.ok".to_string()));
    }

    #[test]
    fn test_edge_basic_creation() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = Edge::new(from, to);

        assert_eq!(edge.from, from);
        assert_eq!(edge.to, to);
        assert_eq!(edge.label, None);
        assert_eq!(edge.condition, None);
        assert_ne!(edge.id, Uuid::nil());
    }

    #[test]
    fn test_edge_with_label_only() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = Edge::new(from, to).with_label("branch_a".to_string());

        assert_eq!(edge.label, Some("branch_a".to_string()));
        assert_eq!(edge.condition, None);
    }

    #[test]
    fn test_edge_with_condition_only() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = Edge::new(from, to).with_condition("x > 10".to_string());

        assert_eq!(edge.condition, Some("x > 10".to_string()));
        assert_eq!(edge.label, None);
    }

    #[test]
    fn test_edge_unique_ids() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();

        let edge1 = Edge::new(from, to);
        let edge2 = Edge::new(from, to);

        assert_ne!(edge1.id, edge2.id);
    }

    #[test]
    fn test_edge_builder_pattern_chaining() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();

        let edge = Edge::new(from, to)
            .with_label("test".to_string())
            .with_condition("true".to_string());

        assert_eq!(edge.label, Some("test".to_string()));
        assert_eq!(edge.condition, Some("true".to_string()));
        assert_eq!(edge.from, from);
        assert_eq!(edge.to, to);
    }

    #[test]
    fn test_edge_serialization() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let edge = Edge::new(from, to).with_label("success".to_string());

        let json = serde_json::to_string(&edge).unwrap();
        let deserialized: Edge = serde_json::from_str(&json).unwrap();

        assert_eq!(edge.id, deserialized.id);
        assert_eq!(edge.from, deserialized.from);
        assert_eq!(edge.to, deserialized.to);
        assert_eq!(edge.label, deserialized.label);
        assert_eq!(edge.condition, deserialized.condition);
    }
}
