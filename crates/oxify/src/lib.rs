//! OxiFY - LLM Workflow Orchestration Platform with DAG-based pipelines.
//!
//! This is the meta crate that re-exports all OxiFY components for convenient access.
//!
//! # Overview
//!
//! OxiFY is a comprehensive platform for building, executing, and managing
//! LLM-powered workflows as directed acyclic graphs (DAGs). This crate provides
//! a unified API to all OxiFY components:
//!
//! - **[`model`]**: Domain models for workflows, nodes, and edges
//! - **[`vector`]**: High-performance vector search with SIMD acceleration
//! - **[`authn`]**: Authentication services (OAuth, API keys, JWT)
//! - **[`authz`]**: Authorization and access control
//! - **[`server`]**: HTTP server infrastructure
//! - **[`mcp`]**: Model Context Protocol implementation
//! - **[`connect_llm`]**: LLM provider connectors (OpenAI, Anthropic, etc.)
//! - **[`connect_vector`]**: Vector database connectors (Qdrant, etc.)
//! - **[`connect_vision`]**: Vision model connectors
//! - **[`storage`]**: Persistence layer
//! - **[`engine`]**: Workflow execution engine
//!
//! # Quick Start
//!
//! ```ignore
//! use oxify::prelude::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Build a workflow
//!     let workflow = WorkflowBuilder::new("my-workflow")
//!         .description("A sample LLM workflow")
//!         .build()?;
//!
//!     // Execute the workflow
//!     let engine = ExecutionEngine::new();
//!     let result = engine.execute(&workflow).await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Module Structure
//!
//! | Module | Description |
//! |--------|-------------|
//! | [`model`] | Domain models for workflows, nodes, edges, and execution |
//! | [`vector`] | Vector similarity search with HNSW indexing |
//! | [`authn`] | Authentication (OAuth2, API keys, JWT tokens) |
//! | [`authz`] | Authorization and role-based access control |
//! | [`server`] | HTTP/WebSocket server infrastructure |
//! | [`mcp`] | Model Context Protocol for tool integration |
//! | [`connect_llm`] | LLM provider integrations |
//! | [`connect_vector`] | Vector database integrations |
//! | [`connect_vision`] | Vision model integrations |
//! | [`storage`] | Persistent storage layer |
//! | [`engine`] | Workflow execution engine |

#![cfg_attr(docsrs, feature(doc_cfg))]

/// Re-export of `oxify-model` - Domain models for workflows.
pub use oxify_model as model;

/// Re-export of `oxify-vector` - Vector similarity search.
pub use oxify_vector as vector;

/// Re-export of `oxify-authn` - Authentication services.
pub use oxify_authn as authn;

/// Re-export of `oxify-authz` - Authorization services.
pub use oxify_authz as authz;

/// Re-export of `oxify-server` - HTTP server infrastructure.
pub use oxify_server as server;

/// Re-export of `oxify-mcp` - Model Context Protocol.
pub use oxify_mcp as mcp;

/// Re-export of `oxify-connect-llm` - LLM connectors.
pub use oxify_connect_llm as connect_llm;

/// Re-export of `oxify-connect-vector` - Vector DB connectors.
pub use oxify_connect_vector as connect_vector;

/// Re-export of `oxify-connect-vision` - Vision model connectors.
pub use oxify_connect_vision as connect_vision;

/// Re-export of `oxify-storage` - Persistence layer.
pub use oxify_storage as storage;

/// Re-export of `oxify-engine` - Workflow execution engine.
pub use oxify_engine as engine;

/// Prelude module for convenient imports.
///
/// Import everything commonly needed with:
/// ```ignore
/// use oxify::prelude::*;
/// ```
pub mod prelude {
    // ========================================
    // From oxify-model
    // ========================================

    // Workflow building
    pub use oxify_model::{NodeBuilder, WorkflowBuilder};

    // Core types
    pub use oxify_model::workflow::Workflow;
    pub use oxify_model::{Edge, EdgeId, Node, NodeId, NodeKind};

    // Execution
    pub use oxify_model::{
        ExecutionContext, ExecutionResult, ExecutionState, NodeExecutionResult, TokenUsage,
    };

    // Events
    pub use oxify_model::{EventDetails, EventId, EventType, ExecutionEvent, ExecutionId};

    // Configuration
    pub use oxify_model::{
        CacheConfig, CheckpointConfig, LlmConfig, McpConfig, RetryConfig, TimeoutConfig,
        VectorConfig, VisionConfig,
    };

    // Analytics
    pub use oxify_model::{
        AnalyticsBuilder, ExecutionStats, NodeAnalytics, PerformanceMetrics, WorkflowAnalytics,
    };

    // Cost estimation
    pub use oxify_model::{CostEstimate, CostEstimator, ModelPricing};

    // Optimization
    pub use oxify_model::{OptimizationReport, OptimizationSuggestion, WorkflowOptimizer};

    // Validation
    pub use oxify_model::validation::{ValidationError, ValidationReport, WorkflowValidator};

    // ========================================
    // From oxify-vector
    // ========================================

    pub use oxify_vector::{DistanceMetric, HnswConfig, HnswIndex, HnswStats, SearchResult};

    // ========================================
    // From oxify-engine
    // ========================================

    pub use oxify_engine::{Engine, EngineBuilder, EngineError, ExecutionConfig};

    // ========================================
    // From oxify-connect-llm
    // ========================================

    pub use oxify_connect_llm::{LlmError, LlmProvider, LlmRequest, LlmResponse};

    // ========================================
    // From oxify-mcp
    // ========================================

    pub use oxify_mcp::{McpClient, McpRequest, McpResponse, McpServer, ToolSchema};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_access() {
        // Verify all modules are accessible
        let _ = model::NodeKind::LLM;
    }
}
