//! OxiFY Model - Domain models for LLM workflow orchestration
//!
//! This crate provides the core data structures for defining and managing
//! LLM workflows as directed acyclic graphs (DAGs).

pub mod analytics;
pub mod batching;
pub mod builder;
pub mod cache;
pub mod checkpoint;
pub mod cost;
pub mod edge;
pub mod event;
pub mod execution;
pub mod graphql_schema;
pub mod http_util;
pub mod json_schema;
pub mod linter;
pub mod metrics_export;
pub mod node;
pub mod optimizer;
pub mod prediction;
#[cfg(all(feature = "python", target_os = "linux"))]
pub mod python;
pub mod rollback;
pub mod schedule;
pub mod schema;
pub mod secret;
pub mod security;
pub mod simulator;
pub mod template;
pub mod test_utils;
pub mod typescript;
pub mod validation;
pub mod variable_optimizer;
pub mod versioning;
pub mod visualization;
#[cfg(feature = "wasm")]
pub mod wasm;
pub mod webhook;
pub mod webhook_transform;
pub mod workflow;
pub mod yaml;

pub use analytics::{
    AnalyticsBuilder, AnalyticsPeriod, ErrorPattern, ErrorTrend, ExecutionStats, NodeAnalytics,
    PerformanceMetrics, PeriodType, WorkflowAnalytics,
};

pub use batching::{BatchAnalyzer, BatchOpportunity, BatchPlan, BatchStats, ExecutionBatch};
pub use builder::{NodeBuilder, WorkflowBuilder};
pub use cache::{
    CacheConfig, CacheEntry, CacheKeyGenerator, CacheManager, CachePolicy, CacheStats,
    CacheWarmingConfig, InvalidationPlan, InvalidationStrategy, WarmingStrategy,
};
pub use checkpoint::{
    CheckpointConfig, CheckpointError, CheckpointFrequency, CheckpointId, CheckpointMetadata,
    CheckpointStorage, InMemoryCheckpointStorage,
};
pub use cost::{
    CategoryCosts, CostComponent, CostEstimate, CostEstimator, ModelPricing, NodeCost,
    TokenEstimates,
};
pub use edge::{Edge, EdgeId};
pub use event::{EventDetails, EventId, EventTimeline, EventType, ExecutionEvent, ExecutionId};
pub use execution::{
    ExecutionCheckpoint, ExecutionContext, ExecutionResult, ExecutionState, NodeExecutionResult,
    NodeMetrics, TokenUsage,
};
pub use graphql_schema::{
    generate_graphql_schema, GraphQLArgument, GraphQLField, GraphQLSchemaGenerator, GraphQLType,
    GraphQLTypeKind,
};
pub use http_util::append_query_params;
pub use json_schema::{
    generate_workflow_schema, schema_to_json, schema_to_value, JsonSchema, WorkflowSchemaGenerator,
};
pub use linter::{
    LintCategory, LintFinding, LintResult, LintSeverity, LintStats, LinterConfig, WorkflowLinter,
};
pub use metrics_export::{ExportFormat, MetricsError, MetricsExporter};
pub use node::{
    ApprovalConfig, Condition, CustomConfig, FormConfig, FormField, FormFieldType, LlmConfig,
    LoopConfig, LoopType, McpConfig, Node, NodeId, NodeKind, ParallelConfig, ParallelStrategy,
    ParallelTask, RetryConfig, ScriptConfig, SubWorkflowConfig, SwitchCase, SwitchConfig,
    TimeoutAction, TimeoutConfig, TryCatchConfig, VectorConfig, VisionConfig,
};
pub use optimizer::{
    Benefit, ComplexityMetrics, ImprovementSummary, IssueType, OptimizationReport,
    OptimizationSuggestion, Severity, SuggestionCategory, WorkflowIssue, WorkflowOptimizer,
};
pub use prediction::{HistoricalData, NodeTime, TimeEstimate, TimePredictor};
pub use rollback::{
    ExecutionSnapshot, RollbackManager, RollbackResult, RollbackSummary, SnapshotMetadata,
};
pub use schedule::{Schedule, ScheduleExecution, ScheduleId};
pub use schema::{
    BackwardCompatibility, DeprecatedField, FieldMigration, MigrationRegistry, ModelMetadata,
    PreservedFields, SchemaMigration, SchemaVersion, Versioned, VersionedWithCompat,
    CURRENT_SCHEMA_VERSION,
};
pub use secret::{
    AccessControl, CreateSecretRequest, EncryptionMetadata, Secret, SecretAction, SecretAuditLog,
    SecretId, SecretReference, SecretView, UpdateSecretRequest,
};
pub use security::{
    ComplianceStandard, RiskLevel, RiskSummary, SecurityAuditReport, SecurityConfig,
    SecurityFinding, SecurityScanner, ThreatCategory,
};
pub use simulator::{
    CoverageInfo, ExecutionTrace, NodeExecutionDetail, SimulationError, SimulationResult,
    WorkflowSimulator,
};
pub use template::{
    InstantiateTemplateRequest, ParameterType, ParameterValidation, TemplateId, TemplateListItem,
    TemplateParameter, WorkflowTemplate,
};
pub use typescript::generate_typescript_definitions;
pub use validation::{ValidationError, ValidationReport, ValidationStats, WorkflowValidator};
pub use variable_optimizer::{
    VariableFlow, VariableOptimization, VariableOptimizer, VariableUsage,
};
pub use versioning::{
    ChangeType, ChangelogEntry, ChangelogType, EdgeInfo, MetadataChange, NodeChange,
    VersionCompatibility, WorkflowDiff, WorkflowVersionEntry, WorkflowVersionHistory,
};
pub use visualization::{
    workflow_to_graphviz, workflow_to_mermaid, workflow_to_plantuml, DiagramOrientation,
    VisualizationFormat, VisualizationStyle, WorkflowVisualizer,
};
pub use webhook::{
    create_webhook_signature, generate_webhook_secret, verify_webhook_signature,
    CreateWebhookRequest, UpdateWebhookRequest, Webhook, WebhookEvent, WebhookEventStatus,
    WebhookId, WebhookRegistrationResponse, WebhookTriggerRequest, WebhookView,
};
pub use webhook_transform::{
    FieldMapping, PayloadTransform, PayloadTransformError, StringTransformType, TransformOperation,
    TransformPipeline,
};
pub use workflow::{VersionBump, Workflow, WorkflowId, WorkflowMetadata, WorkflowSchedule};
pub use yaml::{
    json_to_yaml, load_template_yaml, load_workflow_yaml, save_template_yaml, save_workflow_yaml,
    template_from_yaml, template_to_yaml, workflow_from_yaml, workflow_to_yaml, yaml_to_json,
    YamlError,
};
