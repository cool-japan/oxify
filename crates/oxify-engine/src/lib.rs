//! OxiFY Engine - DAG execution engine for LLM workflows

mod api_documentation;
mod approval_store;
mod backpressure;
mod batching;
mod chaos_testing;
mod checkpoint;
mod code_executor;
mod conditional;
mod cost_estimator;
mod event_bus;
mod executor;
mod form_store;
mod graceful_degradation;
mod health_checker;
mod llm_cache;
mod loop_executor;
mod mcp_executor;
mod metrics;
mod node_executor;
mod optimizer;
mod otel;
mod plan_cache;
mod plugin;
mod plugin_dependencies;
mod plugin_loader;
mod plugin_manifest;
mod plugin_marketplace;
mod plugin_security;
mod plugin_wasm;
mod profiler;
mod resource_limits;
mod resource_monitor;
mod rest_connector;
mod retry;
mod scheduler;
mod subworkflow_executor;
mod testing;
mod try_catch_executor;
mod variable_store;
mod visualization;
mod webhook;
mod websocket_connector;
mod workflow_analyzer;
mod workflow_template;

pub use api_documentation::{
    ApiDocGenerator, ApiEndpoint, ApiParameterType, JsonSchema, OAuthFlow, Parameter,
    SecurityScheme,
};
pub use approval_store::{ApprovalId, ApprovalRequest, ApprovalStatus, ApprovalStore};
pub use backpressure::{
    BackpressureConfig, BackpressureMonitor, BackpressureState, BackpressureStats,
    BackpressureStrategy,
};
pub use batching::{Batch, BatchAnalyzer, BatchGroup, BatchPlan, BatchStats};
pub use chaos_testing::{
    ChaosConfig, ChaosEngine, ChaosFailure, ChaosMonkey, ChaosReport, CpuThrottleEvent,
    DiskIoFailureEvent, LatencyInjection, MemoryPressureEvent,
};
pub use checkpoint::{
    CheckpointId, CheckpointStore, ExecutionCheckpoint, FileCheckpointStore,
    InMemoryCheckpointStore,
};
use code_executor::CodeExecutor;
use conditional::ConditionalEvaluator;
pub use cost_estimator::{
    CostEstimator, CostOperation, ExecutionCostEstimate, LlmPricing, NodeCost,
};
pub use event_bus::{
    execution_events, EventBus, EventHandler, EventType, WorkflowEvent, WorkflowTrigger,
};
pub mod event_store;
pub use event_store::{
    EventRecorder, EventStore, EventStoreError, FileEventStore, InMemoryEventStore, StoredEvent,
};
pub use form_store::{FormId, FormStatus, FormStore, FormSubmissionRequest};
pub use graceful_degradation::{
    DegradationAnalyzer, DegradationConfig, DegradationStats, DependencyStrategy, FailedNode,
    PartialResult,
};
pub use health_checker::{HealthChecker, HealthIssue, HealthReport, HealthSeverity};
use llm_cache::EngineLlmCache;
use loop_executor::LoopExecutor;
use mcp_executor::McpExecutor;
pub use metrics::{ExecutionInfo, ExecutionMetrics, ExecutionStats};
pub use optimizer::{Impact, Optimization, OptimizationCategory, Priority, WorkflowOptimizer};
#[cfg(feature = "otel")]
pub use otel::build_otel_provider;
pub use otel::{
    init_tracing, shutdown_tracing, trace_node_async, trace_workflow_async, TracingConfig,
};
use oxify_connect_llm::{
    AnthropicProvider, EmbeddingProvider, EmbeddingRequest, ImageInput, LlmProvider, LlmRequest,
    OllamaProvider, OpenAIProvider, Tool,
};
use oxify_connect_vector::{
    ChromaDBProvider, MilvusProvider, PineconeProvider, QdrantProvider, SearchRequest,
    VectorProvider, WeaviateProvider,
};
use oxify_connect_vision::{create_provider, VisionProviderConfig};
use oxify_model::{
    ExecutionContext, ExecutionResult, ExecutionState, Node, NodeExecutionResult, NodeId, NodeKind,
    ParallelStrategy, ParallelTask, Workflow,
};
use plan_cache::{ExecutionPlan, PlanCache};
pub use plugin::{DatabaseNodePlugin, HttpNodePlugin, NodePlugin, PluginMetadata, PluginRegistry};
pub use plugin_dependencies::{
    DependencyError, DependencyGraph, DependencyNode, DependencyResolver,
};
pub use plugin_loader::{
    PluginLoadResult, PluginLoader, PluginLoaderConfig, PluginLoaderError, PluginLoaderStats,
};
pub use plugin_manifest::{
    LoadedPlugin, ManifestError, PluginCapabilities, PluginCategory, PluginConfig, PluginHooks,
    PluginInfo, PluginManager, PluginManifest, PluginState, PluginStats, ResourceRequirements,
};
pub use plugin_marketplace::{
    MarketplaceError, MarketplaceManager, RegistryClient, RegistryConfig, SearchCriteria,
    SearchResult,
};
pub use plugin_security::{
    PluginSecurityScanner, SecurityCategory, SecurityError, SecurityIssue, SecurityPolicy,
    SecurityScanResult, SecurityWarning,
};
#[cfg(feature = "wasm")]
pub use plugin_wasm::WasmNodePlugin;
pub use plugin_wasm::{WasmError, WasmPlugin, WasmPluginConfig, WasmPluginLoader, WasmPluginStats};
pub use profiler::{
    BottleneckType, NodeProfile, PerformanceAnalyzer, PerformanceBottleneck, PerformanceSummary,
    WorkflowProfile,
};
pub use resource_limits::{
    DetailedResourceUsage, LimitsBuilder, NodeResourceUsage, ResourceEnforcer, ResourceLimitError,
    ResourceLimits, ResourceMonitor as ResourceLimitsMonitor,
    ResourceUsage as ExecutionResourceUsage, ResourceUsageHistory, ResourceWarning, TokenBudget,
    TokenReservation, UsageGrowthRates, UsageHistoryEntry, UsageSnapshot, UserQuota,
    UserQuotaManager,
};
pub use resource_monitor::{
    ResourceConfig, ResourceMonitor, ResourceStats, ResourceThresholds, ResourceUsage,
    SchedulingPolicy,
};
pub use rest_connector::{
    AuthConfig, CircuitBreaker, CircuitBreakerConfig, CircuitState, GraphQLQuery,
    HeaderInjectionInterceptor, LoggingInterceptor, RateLimitConfig, RequestInterceptor,
    RequestTemplate, ResponseInterceptor, RestConfig, RestConnector, RestConnectorError,
    RestResponse, RetryConfig,
};
pub use retry::retry_with_backoff;
pub use scheduler::{ScheduleId, ScheduledExecution, WorkflowScheduler};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use subworkflow_executor::SubWorkflowExecutor;
pub use testing::{
    AssertionResult, ExpectedStatus, ExpectedValue, TestReport, TestResult, TestSuite,
    WorkflowTestCase, WorkflowTestRunner,
};
use thiserror::Error;
use try_catch_executor::TryCatchExecutor;
pub use variable_store::{Variable, VariableStore, VariableStoreStats};
pub use visualization::{
    export_to_ascii, export_to_dot, export_to_mermaid, VisualizationFormat, WorkflowVisualizer,
};
pub use webhook::{WebhookConfig, WebhookId, WebhookRegistry, WebhookTrigger};
#[cfg(feature = "nats")]
pub mod nats_bridge;
#[cfg(feature = "nats")]
pub use nats_bridge::{NatsBridge, NatsBridgeConfig, NatsBridgeError, NatsCreds};

#[cfg(feature = "kafka")]
pub mod kafka_bridge;
#[cfg(feature = "kafka")]
pub use kafka_bridge::{KafkaBridge, KafkaBridgeConfig, KafkaBridgeError, KafkaCreds};

#[cfg(feature = "rabbitmq")]
pub mod rabbitmq_bridge;
#[cfg(feature = "rabbitmq")]
pub use rabbitmq_bridge::{RabbitMqBridge, RabbitMqBridgeConfig, RabbitMqBridgeError};
#[cfg(feature = "websocket")]
pub use websocket_connector::{
    ConnectionState, WebSocketConfig, WebSocketConnector, WebSocketError, WebSocketMessage,
};
pub use workflow_analyzer::{
    ComplexityLevel, ComplexityMetrics, OptimizationRecommendation, OptimizationType,
    WorkflowAnalyzer,
};
pub use workflow_template::{
    ParameterDef, ParameterType, TemplateError, ValidationRule, WorkflowTemplate,
};

pub type Result<T> = std::result::Result<T, EngineError>;

// Global LLM cache
static LLM_CACHE: LazyLock<EngineLlmCache> = LazyLock::new(EngineLlmCache::new);

// Global execution plan cache
static PLAN_CACHE: LazyLock<PlanCache> = LazyLock::new(PlanCache::new);

// Global MCP executor
static MCP_EXECUTOR: LazyLock<McpExecutor> = LazyLock::new(McpExecutor::new);

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Workflow validation failed: {0}")]
    ValidationError(String),

    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),

    #[error("Execution error: {0}")]
    ExecutionError(String),

    #[error("Cycle detected in workflow")]
    CycleDetected,

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("Template error: {0}")]
    TemplateError(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Execution paused")]
    ExecutionPaused,

    #[error("Execution cancelled")]
    ExecutionCancelled,
}

/// Checkpoint frequency configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CheckpointFrequency {
    /// Never create automatic checkpoints
    #[default]
    Never,
    /// Create checkpoint after every level
    EveryLevel,
    /// Create checkpoint every N levels
    EveryNLevels(usize),
    /// Create checkpoint every N nodes completed
    EveryNNodes(usize),
}

/// Execution configuration
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Whether to emit events during execution
    pub emit_events: bool,

    /// Checkpoint frequency
    pub checkpoint_frequency: CheckpointFrequency,

    /// Per-node timeout in milliseconds (None = no timeout)
    pub node_timeout_ms: Option<u64>,

    /// Maximum concurrent nodes per level (None = unlimited)
    pub max_concurrent_nodes: Option<usize>,

    /// Whether to continue execution on non-critical errors
    pub continue_on_error: bool,

    /// Backpressure configuration (None = no backpressure)
    pub backpressure_config: Option<BackpressureConfig>,

    /// Resource-aware scheduling configuration (None = no resource monitoring)
    pub resource_config: Option<ResourceConfig>,

    /// Resource limits configuration (None = no resource limits)
    pub resource_limits: Option<ResourceLimits>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            emit_events: false,
            checkpoint_frequency: CheckpointFrequency::Never,
            node_timeout_ms: None,
            max_concurrent_nodes: None,
            continue_on_error: false,
            backpressure_config: None,
            resource_config: None,
            resource_limits: None,
        }
    }
}

impl ExecutionConfig {
    /// Create a new execution config
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable event emission
    pub fn with_events(mut self) -> Self {
        self.emit_events = true;
        self
    }

    /// Set checkpoint frequency
    pub fn with_checkpoint_frequency(mut self, frequency: CheckpointFrequency) -> Self {
        self.checkpoint_frequency = frequency;
        self
    }

    /// Set per-node timeout
    pub fn with_node_timeout(mut self, timeout_ms: u64) -> Self {
        self.node_timeout_ms = Some(timeout_ms);
        self
    }

    /// Set maximum concurrent nodes
    pub fn with_max_concurrent(mut self, max: usize) -> Self {
        self.max_concurrent_nodes = Some(max);
        self
    }

    /// Enable continue on error
    pub fn with_continue_on_error(mut self) -> Self {
        self.continue_on_error = true;
        self
    }

    /// Set backpressure configuration
    pub fn with_backpressure(mut self, config: BackpressureConfig) -> Self {
        self.backpressure_config = Some(config);
        self
    }

    /// Set resource-aware scheduling configuration
    pub fn with_resource_monitoring(mut self, config: ResourceConfig) -> Self {
        self.resource_config = Some(config);
        self
    }

    /// Set resource limits configuration
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = Some(limits);
        self
    }
}

/// Builder for creating configured Engine instances
///
/// # Example
/// ```ignore
/// let engine = EngineBuilder::new()
///     .with_checkpoint_store(checkpoint_store)
///     .with_event_bus(event_bus)
///     .with_approval_store(approval_store)
///     .with_metrics()
///     .build();
/// ```
#[derive(Default)]
pub struct EngineBuilder {
    approval_store: Option<ApprovalStore>,
    form_store: Option<FormStore>,
    checkpoint_store: Option<Arc<dyn CheckpointStore>>,
    event_bus: Option<Arc<EventBus>>,
    metrics: Option<Arc<ExecutionMetrics>>,
    plugin_registry: Option<Arc<PluginRegistry>>,
}

impl EngineBuilder {
    /// Create a new EngineBuilder
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an approval store for human-in-the-loop workflows
    pub fn with_approval_store(mut self, store: ApprovalStore) -> Self {
        self.approval_store = Some(store);
        self
    }

    /// Add a form store for human-in-the-loop workflows
    pub fn with_form_store(mut self, store: FormStore) -> Self {
        self.form_store = Some(store);
        self
    }

    /// Add a checkpoint store for pause/resume and recovery
    pub fn with_checkpoint_store(mut self, store: Arc<dyn CheckpointStore>) -> Self {
        self.checkpoint_store = Some(store);
        self
    }

    /// Add an event bus for execution events
    pub fn with_event_bus(mut self, bus: Arc<EventBus>) -> Self {
        self.event_bus = Some(bus);
        self
    }

    /// Enable metrics collection (creates a new metrics instance)
    pub fn with_metrics(mut self) -> Self {
        self.metrics = Some(Arc::new(ExecutionMetrics::new()));
        self
    }

    /// Add a shared metrics collector
    pub fn with_shared_metrics(mut self, metrics: Arc<ExecutionMetrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Register a plugin registry for custom node dispatch
    pub fn with_plugin_registry(mut self, registry: Arc<PluginRegistry>) -> Self {
        self.plugin_registry = Some(registry);
        self
    }

    /// Build the Engine instance
    pub fn build(self) -> Engine {
        Engine {
            approval_store: self.approval_store,
            form_store: self.form_store,
            checkpoint_store: self.checkpoint_store,
            event_bus: self.event_bus,
            metrics: self.metrics,
            plugin_registry: self
                .plugin_registry
                .unwrap_or_else(|| Arc::new(PluginRegistry::new())),
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }
}

/// DAG execution engine
pub struct Engine {
    /// Approval store for human-in-the-loop workflows
    pub(crate) approval_store: Option<ApprovalStore>,

    /// Form store for human-in-the-loop workflows
    pub(crate) form_store: Option<FormStore>,

    /// Checkpoint store for pause/resume and recovery
    pub(crate) checkpoint_store: Option<Arc<dyn CheckpointStore>>,

    /// Event bus for publishing execution events
    pub(crate) event_bus: Option<Arc<EventBus>>,

    /// Metrics collector
    pub(crate) metrics: Option<Arc<ExecutionMetrics>>,

    /// Plugin registry for custom node dispatch
    pub(crate) plugin_registry: Arc<PluginRegistry>,

    /// Pause control flag
    pub(crate) pause_flag: Arc<std::sync::RwLock<HashMap<uuid::Uuid, bool>>>,

    /// Cancel control flag
    pub(crate) cancel_flag: Arc<std::sync::RwLock<HashMap<uuid::Uuid, bool>>>,
}

impl Engine {
    /// Create a new Engine with default configuration
    pub fn new() -> Self {
        Self {
            approval_store: None,
            form_store: None,
            checkpoint_store: None,
            event_bus: None,
            metrics: None,
            plugin_registry: Arc::new(PluginRegistry::new()),
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Create an EngineBuilder for fluent configuration
    pub fn builder() -> EngineBuilder {
        EngineBuilder::new()
    }

    /// Create a new engine with approval store enabled
    pub fn with_approval_store(approval_store: ApprovalStore) -> Self {
        Self {
            approval_store: Some(approval_store),
            form_store: None,
            checkpoint_store: None,
            event_bus: None,
            metrics: None,
            plugin_registry: Arc::new(PluginRegistry::new()),
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Create a new engine with checkpoint store enabled
    pub fn with_checkpoint_store(checkpoint_store: Arc<dyn CheckpointStore>) -> Self {
        Self {
            approval_store: None,
            form_store: None,
            checkpoint_store: Some(checkpoint_store),
            event_bus: None,
            metrics: None,
            plugin_registry: Arc::new(PluginRegistry::new()),
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Create a new engine with event bus enabled
    pub fn with_event_bus(event_bus: Arc<EventBus>) -> Self {
        Self {
            approval_store: None,
            form_store: None,
            checkpoint_store: None,
            event_bus: Some(event_bus),
            metrics: None,
            plugin_registry: Arc::new(PluginRegistry::new()),
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Create a new engine with both approval and checkpoint stores
    pub fn with_stores(
        approval_store: ApprovalStore,
        checkpoint_store: Arc<dyn CheckpointStore>,
    ) -> Self {
        Self {
            approval_store: Some(approval_store),
            form_store: None,
            checkpoint_store: Some(checkpoint_store),
            event_bus: None,
            metrics: None,
            plugin_registry: Arc::new(PluginRegistry::new()),
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    /// Set the event bus for this engine
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Get reference to the event bus if available
    pub fn event_bus(&self) -> Option<&Arc<EventBus>> {
        self.event_bus.as_ref()
    }

    /// Set the metrics collector for this engine
    pub fn set_metrics(&mut self, metrics: Arc<ExecutionMetrics>) {
        self.metrics = Some(metrics);
    }

    /// Get reference to the metrics collector if available
    pub fn metrics(&self) -> Option<&Arc<ExecutionMetrics>> {
        self.metrics.as_ref()
    }

    /// Get execution statistics (returns default stats if metrics not enabled)
    pub fn get_stats(&self) -> ExecutionStats {
        self.metrics
            .as_ref()
            .map(|m| m.get_stats())
            .unwrap_or_default()
    }

    /// Emit an event if the event bus is configured
    pub(crate) async fn emit_event(&self, event: WorkflowEvent) {
        if let Some(event_bus) = &self.event_bus {
            let _ = event_bus.publish(event).await;
        }
    }

    /// Pause an execution
    pub fn pause_execution(&self, execution_id: uuid::Uuid) {
        self.pause_flag
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(execution_id, true);
        tracing::info!("Paused execution {}", execution_id);
    }

    /// Resume an execution
    pub fn resume_execution(&self, execution_id: uuid::Uuid) {
        self.pause_flag
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(execution_id, false);
        tracing::info!("Resumed execution {}", execution_id);
    }

    /// Check if an execution is paused
    pub fn is_paused(&self, execution_id: uuid::Uuid) -> bool {
        self.pause_flag
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&execution_id)
            .copied()
            .unwrap_or(false)
    }

    /// Cancel an execution
    pub fn cancel_execution(&self, execution_id: uuid::Uuid) {
        self.cancel_flag
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(execution_id, true);
        tracing::warn!("Cancelled execution {}", execution_id);
    }

    /// Check if an execution is cancelled
    pub fn is_cancelled(&self, execution_id: uuid::Uuid) -> bool {
        self.cancel_flag
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&execution_id)
            .copied()
            .unwrap_or(false)
    }

    /// Clear cancellation flag for an execution
    pub fn clear_cancellation(&self, execution_id: uuid::Uuid) {
        self.cancel_flag
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&execution_id);
    }

    /// Create a checkpoint
    pub fn create_checkpoint(
        &self,
        workflow_id: uuid::Uuid,
        execution_id: uuid::Uuid,
        context: &ExecutionContext,
        completed_nodes: Vec<NodeId>,
        current_level: usize,
        reason: String,
    ) -> Result<CheckpointId> {
        if let Some(store) = &self.checkpoint_store {
            let checkpoint = ExecutionCheckpoint::new(
                workflow_id,
                execution_id,
                context.clone(),
                completed_nodes,
                current_level,
                reason,
            );

            store.save(&checkpoint).map_err(EngineError::ExecutionError)
        } else {
            Err(EngineError::ExecutionError(
                "Checkpoint store not configured".to_string(),
            ))
        }
    }

    /// Resume from checkpoint
    pub async fn resume_from_checkpoint(
        &self,
        checkpoint_id: CheckpointId,
        workflow: &Workflow,
    ) -> Result<ExecutionContext> {
        let checkpoint = if let Some(store) = &self.checkpoint_store {
            store
                .load(checkpoint_id)
                .map_err(EngineError::ExecutionError)?
        } else {
            return Err(EngineError::ExecutionError(
                "Checkpoint store not configured".to_string(),
            ));
        };

        tracing::info!(
            "Resuming execution {} from checkpoint {}",
            checkpoint.execution_id,
            checkpoint_id
        );

        // Mark as not paused
        self.resume_execution(checkpoint.execution_id);

        // Continue execution from checkpoint
        self.execute_from_checkpoint(workflow, checkpoint).await
    }

    /// Resolve template variables like {{variable_name}}
    pub(crate) fn resolve_template(
        &self,
        template: &str,
        ctx: &ExecutionContext,
    ) -> Result<String> {
        let mut result = template.to_string();

        // Simple template resolution - find {{variable}} patterns
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}")
            .map_err(|e| EngineError::TemplateError(e.to_string()))?;

        for cap in re.captures_iter(template) {
            let var_name = cap
                .get(1)
                .expect("invariant: capture group 1 present in regex")
                .as_str()
                .trim();

            // Check in context variables
            if let Some(value) = ctx.variables.get(var_name) {
                result = result.replace(&cap[0], &value.to_string());
            } else {
                // Check in node results
                let parts: Vec<&str> = var_name.split('.').collect();
                if parts.len() == 2 {
                    // Format: node_name.field
                    // For now, just replace with placeholder
                    result = result.replace(&cap[0], &format!("[{}]", var_name));
                } else {
                    return Err(EngineError::VariableNotFound(var_name.to_string()));
                }
            }
        }

        Ok(result)
    }

    /// Perform topological sort on the workflow
    #[allow(dead_code)]
    pub(crate) fn topological_sort(&self, workflow: &Workflow) -> Result<Vec<NodeId>> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut adj_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Initialize
        for node in &workflow.nodes {
            in_degree.insert(node.id, 0);
            adj_list.insert(node.id, Vec::new());
        }

        // Build adjacency list and in-degree
        for edge in &workflow.edges {
            adj_list
                .get_mut(&edge.from)
                .expect("invariant: edge.from populated from workflow.nodes")
                .push(edge.to);
            *in_degree
                .get_mut(&edge.to)
                .expect("invariant: edge.to populated from workflow.nodes") += 1;
        }

        // Kahn's algorithm
        let mut queue: Vec<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::new();

        while let Some(node_id) = queue.pop() {
            result.push(node_id);

            if let Some(neighbors) = adj_list.get(&node_id) {
                for &neighbor in neighbors {
                    let deg = in_degree
                        .get_mut(&neighbor)
                        .expect("invariant: neighbor populated from workflow.nodes");
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push(neighbor);
                    }
                }
            }
        }

        if result.len() != workflow.nodes.len() {
            return Err(EngineError::CycleDetected);
        }

        Ok(result)
    }

    /// Discover plugin manifests under `dir`, security-scan them, and register every
    /// successfully loaded plugin into this engine's plugin registry so
    /// [`NodeKind::Custom`] nodes can dispatch to them.
    pub async fn load_plugins_from(
        &self,
        dir: &std::path::Path,
    ) -> std::result::Result<Vec<PluginLoadResult>, PluginLoaderError> {
        let config = PluginLoaderConfig::default().with_search_path(dir.to_path_buf());
        let loader =
            crate::plugin_loader::PluginLoader::with_registry(config, self.plugin_registry.clone());
        loader.load_all().await
    }

    /// Compute execution levels for parallel execution
    /// Returns a Vec of levels, where each level contains nodes that can execute in parallel
    pub(crate) fn compute_execution_levels(&self, workflow: &Workflow) -> Result<Vec<Vec<NodeId>>> {
        // Compute workflow hash
        let workflow_hash = plan_cache::hash_workflow_structure(&workflow.nodes, &workflow.edges);

        // Check cache first
        if let Some(cached_plan) = PLAN_CACHE.get(&workflow.metadata.id, workflow_hash) {
            return Ok(cached_plan.levels);
        }

        // Cache miss - compute execution levels
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut adj_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Initialize
        for node in &workflow.nodes {
            in_degree.insert(node.id, 0);
            adj_list.insert(node.id, Vec::new());
        }

        // Build adjacency list and in-degree
        for edge in &workflow.edges {
            adj_list
                .get_mut(&edge.from)
                .expect("invariant: edge.from populated from workflow.nodes")
                .push(edge.to);
            *in_degree
                .get_mut(&edge.to)
                .expect("invariant: edge.to populated from workflow.nodes") += 1;
        }

        // BFS with level tracking
        let mut queue: Vec<NodeId> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut levels: Vec<Vec<NodeId>> = Vec::new();
        let mut processed = 0;

        while !queue.is_empty() {
            // All nodes in queue can execute in parallel (same level)
            let current_level = queue.clone();
            levels.push(current_level.clone());
            processed += current_level.len();

            // Prepare next level
            let mut next_queue = Vec::new();

            for node_id in current_level {
                if let Some(neighbors) = adj_list.get(&node_id) {
                    for &neighbor in neighbors {
                        let deg = in_degree
                            .get_mut(&neighbor)
                            .expect("invariant: neighbor populated from workflow.nodes");
                        *deg -= 1;
                        if *deg == 0 {
                            next_queue.push(neighbor);
                        }
                    }
                }
            }

            queue = next_queue;
        }

        if processed != workflow.nodes.len() {
            return Err(EngineError::CycleDetected);
        }

        // Cache the execution plan
        let plan = ExecutionPlan {
            levels: levels.clone(),
            workflow_hash,
        };
        PLAN_CACHE.put(workflow.metadata.id, plan);

        Ok(levels)
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxify_model::{Edge, LlmConfig};

    #[test]
    fn test_topological_sort() {
        let mut workflow = Workflow::new("Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        let engine = Engine::new();
        let sorted = engine.topological_sort(&workflow).unwrap();

        assert_eq!(sorted.len(), 2);
        assert_eq!(sorted[0], start_id);
        assert_eq!(sorted[1], end_id);
    }

    #[tokio::test]
    async fn test_simple_workflow_execution() {
        let mut workflow = Workflow::new("Simple Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let llm = Node::new(
            "LLM".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "openai".to_string(),
                model: "gpt-4".to_string(),
                system_prompt: None,
                prompt_template: "Test prompt".to_string(),
                temperature: Some(0.7),
                max_tokens: Some(100),
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let llm_id = llm.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(llm);
        workflow.add_node(end);

        workflow.add_edge(Edge::new(start_id, llm_id));
        workflow.add_edge(Edge::new(llm_id, end_id));

        let engine = Engine::new();
        let ctx = engine.execute(&workflow).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
        assert_eq!(ctx.node_results.len(), 3);
    }

    #[tokio::test]
    async fn test_execute_with_events() {
        let mut workflow = Workflow::new("Event Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create engine with event bus
        let event_bus = Arc::new(EventBus::new(100));
        let mut rx = event_bus.subscribe();

        let mut engine = Engine::new();
        engine.set_event_bus(event_bus);

        // Execute with events enabled
        let config = ExecutionConfig::new().with_events();
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);

        // Check that we received events
        let mut event_count = 0;
        while let Ok(_event) = rx.try_recv() {
            event_count += 1;
        }

        // We should have received multiple events (workflow.started, level.started, etc.)
        assert!(event_count > 0, "Should have received at least one event");
    }

    #[tokio::test]
    async fn test_execute_with_checkpointing() {
        let mut workflow = Workflow::new("Checkpoint Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create engine with checkpoint store
        let checkpoint_store = Arc::new(InMemoryCheckpointStore::new());
        let engine = Engine::with_checkpoint_store(checkpoint_store.clone());

        // Execute with checkpointing enabled
        let config =
            ExecutionConfig::new().with_checkpoint_frequency(CheckpointFrequency::EveryLevel);

        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);

        // Check that checkpoints were created
        let checkpoints = checkpoint_store.list_by_execution(ctx.execution_id);
        // With 2 levels, we should have 2 checkpoints
        assert!(!checkpoints.is_empty(), "Should have created checkpoints");
    }

    #[test]
    fn test_execution_config_builder() {
        let config = ExecutionConfig::new()
            .with_events()
            .with_checkpoint_frequency(CheckpointFrequency::EveryLevel)
            .with_node_timeout(5000)
            .with_max_concurrent(4)
            .with_continue_on_error();

        assert!(config.emit_events);
        assert_eq!(config.checkpoint_frequency, CheckpointFrequency::EveryLevel);
        assert_eq!(config.node_timeout_ms, Some(5000));
        assert_eq!(config.max_concurrent_nodes, Some(4));
        assert!(config.continue_on_error);
    }

    #[test]
    fn test_checkpoint_frequency() {
        assert_eq!(CheckpointFrequency::default(), CheckpointFrequency::Never);

        // Test different frequency patterns
        let freq = CheckpointFrequency::EveryNLevels(3);
        match freq {
            CheckpointFrequency::EveryNLevels(n) => assert_eq!(n, 3),
            _ => panic!("Wrong variant"),
        }

        let freq = CheckpointFrequency::EveryNNodes(10);
        match freq {
            CheckpointFrequency::EveryNNodes(n) => assert_eq!(n, 10),
            _ => panic!("Wrong variant"),
        }
    }

    #[tokio::test]
    async fn test_execute_with_concurrency_limit() {
        let mut workflow = Workflow::new("Concurrency Test".to_string());

        // Create a workflow with multiple nodes at the same level
        // Using LLM nodes for the parallel tier to avoid multiple Start/End validation errors
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let node1 = Node::new(
            "Node1".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "test".to_string(),
                model: "test".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let node2 = Node::new(
            "Node2".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "test".to_string(),
                model: "test".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let node3 = Node::new(
            "Node3".to_string(),
            NodeKind::LLM(LlmConfig {
                provider: "test".to_string(),
                model: "test".to_string(),
                system_prompt: None,
                prompt_template: "test".to_string(),
                temperature: None,
                max_tokens: None,
                tools: Vec::new(),
                images: Vec::new(),
                extra_params: serde_json::Value::Null,
            }),
        );
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let node1_id = node1.id;
        let node2_id = node2.id;
        let node3_id = node3.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(node1);
        workflow.add_node(node2);
        workflow.add_node(node3);
        workflow.add_node(end);

        // All nodes 1-3 depend on start, end depends on all
        workflow.add_edge(Edge::new(start_id, node1_id));
        workflow.add_edge(Edge::new(start_id, node2_id));
        workflow.add_edge(Edge::new(start_id, node3_id));
        workflow.add_edge(Edge::new(node1_id, end_id));
        workflow.add_edge(Edge::new(node2_id, end_id));
        workflow.add_edge(Edge::new(node3_id, end_id));

        let engine = Engine::new();

        // Execute with max 2 concurrent nodes
        let config = ExecutionConfig::new().with_max_concurrent(2);
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
        assert_eq!(ctx.node_results.len(), 5);
    }

    #[test]
    fn test_engine_builder() {
        // Test basic builder
        let engine = Engine::builder().build();
        assert!(engine.event_bus().is_none());
        assert!(engine.metrics().is_none());

        // Test builder with all options
        let checkpoint_store = Arc::new(InMemoryCheckpointStore::new());
        let event_bus = Arc::new(EventBus::new(100));
        let metrics = Arc::new(ExecutionMetrics::new());

        let engine = Engine::builder()
            .with_checkpoint_store(checkpoint_store)
            .with_event_bus(event_bus.clone())
            .with_shared_metrics(metrics.clone())
            .build();

        assert!(engine.event_bus().is_some());
        assert!(engine.metrics().is_some());

        // Test with_metrics creates new instance
        let engine = Engine::builder().with_metrics().build();
        assert!(engine.metrics().is_some());
    }

    #[tokio::test]
    async fn test_engine_with_metrics() {
        let mut workflow = Workflow::new("Metrics Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create engine with metrics enabled
        let engine = Engine::builder().with_metrics().build();

        // Execute workflow
        let ctx = engine.execute(&workflow).await.unwrap();
        assert_eq!(ctx.state, ExecutionState::Completed);

        // Check stats
        let _stats = engine.get_stats();
        // Note: Basic execution doesn't automatically record metrics yet,
        // but the infrastructure is in place
    }

    #[test]
    fn test_execution_stats_default() {
        let stats = ExecutionStats::default();
        assert_eq!(stats.total_workflows, 0);
        assert_eq!(stats.successful_workflows, 0);
        assert_eq!(stats.failed_workflows, 0);
        assert!((stats.workflow_success_rate - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_backpressure_throttle_strategy() {
        use std::time::Duration;

        let mut workflow = Workflow::new("Backpressure Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create backpressure config with throttle strategy
        let bp_config = BackpressureConfig {
            strategy: BackpressureStrategy::Throttle,
            max_queued_nodes: 1,
            max_active_nodes: 1,
            throttle_delay: Duration::from_millis(10),
            high_water_mark: 0.8,
            low_water_mark: 0.6,
        };

        let config = ExecutionConfig::new().with_backpressure(bp_config);

        let engine = Engine::new();
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
    }

    #[tokio::test]
    async fn test_backpressure_none_strategy() {
        let mut workflow = Workflow::new("No Backpressure Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create backpressure config with None strategy (no limits)
        let bp_config = BackpressureConfig::unlimited();

        let config = ExecutionConfig::new().with_backpressure(bp_config);

        let engine = Engine::new();
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
    }

    #[tokio::test]
    async fn test_backpressure_config_builder() {
        let config = BackpressureConfig::new(BackpressureStrategy::Block, 100, 10);

        assert_eq!(config.strategy, BackpressureStrategy::Block);
        assert_eq!(config.max_queued_nodes, 100);
        assert_eq!(config.max_active_nodes, 10);
    }

    #[tokio::test]
    async fn test_backpressure_strict_config() {
        let config = BackpressureConfig::strict(50, 5);

        assert_eq!(config.strategy, BackpressureStrategy::Block);
        assert_eq!(config.max_queued_nodes, 50);
        assert_eq!(config.max_active_nodes, 5);
        assert_eq!(config.high_water_mark, 0.9);
        assert_eq!(config.low_water_mark, 0.7);
    }

    #[tokio::test]
    async fn test_execution_config_with_backpressure() {
        let bp_config = BackpressureConfig::default();
        let exec_config = ExecutionConfig::new().with_backpressure(bp_config.clone());

        assert!(exec_config.backpressure_config.is_some());
        let stored_config = exec_config.backpressure_config.unwrap();
        assert_eq!(stored_config.strategy, bp_config.strategy);
        assert_eq!(stored_config.max_queued_nodes, bp_config.max_queued_nodes);
    }

    #[tokio::test]
    async fn test_resource_aware_scheduling_balanced() {
        let mut workflow = Workflow::new("Resource Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create resource config with balanced policy
        let res_config = ResourceConfig::balanced(5, 20);
        let config = ExecutionConfig::new().with_resource_monitoring(res_config);

        let engine = Engine::new();
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
    }

    #[tokio::test]
    async fn test_resource_aware_scheduling_cpu_based() {
        let mut workflow = Workflow::new("CPU Resource Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create resource config with CPU-based policy
        let res_config = ResourceConfig::cpu_based(3, 15);
        let config = ExecutionConfig::new().with_resource_monitoring(res_config);

        let engine = Engine::new();
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
    }

    #[tokio::test]
    async fn test_resource_aware_scheduling_fixed() {
        let mut workflow = Workflow::new("Fixed Resource Test".to_string());

        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);

        let start_id = start.id;
        let end_id = end.id;

        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create fixed resource config
        let res_config = ResourceConfig::fixed(10);
        let config = ExecutionConfig::new().with_resource_monitoring(res_config);

        let engine = Engine::new();
        let ctx = engine.execute_with_config(&workflow, config).await.unwrap();

        assert_eq!(ctx.state, ExecutionState::Completed);
    }

    #[test]
    fn test_resource_config_builder() {
        let config = ResourceConfig::balanced(10, 50);
        assert_eq!(config.policy, SchedulingPolicy::Balanced);
        assert_eq!(config.min_concurrency, 10);
        assert_eq!(config.max_concurrency, 50);
    }

    #[test]
    fn test_execution_config_with_resource_monitoring() {
        let res_config = ResourceConfig::default();
        let exec_config = ExecutionConfig::new().with_resource_monitoring(res_config.clone());

        assert!(exec_config.resource_config.is_some());
        let stored_config = exec_config.resource_config.unwrap();
        assert_eq!(stored_config.policy, res_config.policy);
        assert_eq!(stored_config.min_concurrency, res_config.min_concurrency);
    }

    #[test]
    fn test_execution_cancellation_state() {
        let engine = Engine::new();
        let execution_id1 = uuid::Uuid::new_v4();
        let execution_id2 = uuid::Uuid::new_v4();

        // Initially not cancelled
        assert!(!engine.is_cancelled(execution_id1));
        assert!(!engine.is_cancelled(execution_id2));

        // Cancel first execution
        engine.cancel_execution(execution_id1);
        assert!(engine.is_cancelled(execution_id1));
        assert!(!engine.is_cancelled(execution_id2)); // Other execution unaffected

        // Cancel second execution
        engine.cancel_execution(execution_id2);
        assert!(engine.is_cancelled(execution_id1));
        assert!(engine.is_cancelled(execution_id2));

        // Clear first cancellation
        engine.clear_cancellation(execution_id1);
        assert!(!engine.is_cancelled(execution_id1));
        assert!(engine.is_cancelled(execution_id2)); // Second still cancelled
    }

    #[test]
    fn test_cancel_execution_methods() {
        let engine = Engine::new();
        let execution_id = uuid::Uuid::new_v4();

        // Initially not cancelled
        assert!(!engine.is_cancelled(execution_id));

        // Cancel execution
        engine.cancel_execution(execution_id);
        assert!(engine.is_cancelled(execution_id));

        // Clear cancellation
        engine.clear_cancellation(execution_id);
        assert!(!engine.is_cancelled(execution_id));
    }

    #[tokio::test]
    async fn test_execution_config_with_resource_limits() {
        // Create a simple workflow
        let mut workflow = Workflow::new("Resource Test".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));

        // Create resource limits with reasonable settings
        let limits = ResourceLimits {
            max_memory_mb: 1024,
            max_execution_time_secs: 60,
            max_tokens_per_execution: 10000,
            max_tokens_per_call: 1000,
            max_concurrent_nodes: 5,
            max_total_nodes: 100,
            max_api_calls: 50,
            max_retries_per_node: 3,
            warning_threshold_percent: 80,
        };

        // Create execution config with resource limits
        let config = ExecutionConfig::new().with_resource_limits(limits);

        // Verify the config has resource limits set
        assert!(config.resource_limits.is_some());

        // Execute workflow with resource limits
        let engine = Engine::new();
        let result = engine.execute_with_config(&workflow, config).await;

        // Should succeed for simple workflow
        assert!(result.is_ok());

        // Verify resource usage was tracked
        if let Ok(ctx) = result {
            assert!(!ctx.node_results.is_empty());
        }
    }

    // Property-based tests using proptest
    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Generate a random linear workflow (guaranteed to be acyclic)
        fn linear_workflow_strategy() -> impl Strategy<Value = Workflow> {
            (2usize..=20).prop_map(|num_nodes| {
                let mut workflow = Workflow::new(format!("linear_{}", num_nodes));

                let start = Node::new("start".to_string(), NodeKind::Start);
                let start_id = start.id;
                workflow.add_node(start);

                let mut prev_id = start_id;
                for i in 0..num_nodes {
                    let node = Node::new(
                        format!("node_{}", i),
                        NodeKind::LLM(LlmConfig {
                            provider: "test".to_string(),
                            model: "test".to_string(),
                            system_prompt: None,
                            prompt_template: format!("Task {}", i),
                            temperature: Some(0.7),
                            max_tokens: Some(100),
                            tools: Vec::new(),
                            images: Vec::new(),
                            extra_params: serde_json::Value::Null,
                        }),
                    );
                    let node_id = node.id;
                    workflow.add_node(node);
                    workflow.add_edge(Edge::new(prev_id, node_id));
                    prev_id = node_id;
                }

                let end = Node::new("end".to_string(), NodeKind::End);
                let end_id = end.id;
                workflow.add_node(end);
                workflow.add_edge(Edge::new(prev_id, end_id));

                workflow
            })
        }

        proptest! {
            /// Property: Topological sort should produce a valid ordering for any linear workflow
            #[test]
            fn prop_topological_sort_valid_ordering(workflow in linear_workflow_strategy()) {
                let engine = Engine::new();
                let sorted = engine.topological_sort(&workflow);

                // Must succeed for acyclic graphs
                prop_assert!(sorted.is_ok());

                let sorted = sorted.unwrap();

                // All nodes must be present
                prop_assert_eq!(sorted.len(), workflow.nodes.len());

                // Start must be first, End must be last
                let start_node = workflow.nodes.iter().find(|n| matches!(n.kind, NodeKind::Start)).unwrap();
                let end_node = workflow.nodes.iter().find(|n| matches!(n.kind, NodeKind::End)).unwrap();

                prop_assert_eq!(sorted.first(), Some(&start_node.id));
                prop_assert_eq!(sorted.last(), Some(&end_node.id));

                // Each node must appear exactly once
                let mut seen = std::collections::HashSet::new();
                for node_id in &sorted {
                    prop_assert!(seen.insert(node_id), "Duplicate node in sort: {:?}", node_id);
                }
            }

            /// Property: All edges must respect topological order
            #[test]
            fn prop_topological_sort_respects_edges(workflow in linear_workflow_strategy()) {
                let engine = Engine::new();
                let sorted = engine.topological_sort(&workflow).unwrap();

                // Build position map
                let positions: std::collections::HashMap<_, _> = sorted
                    .iter()
                    .enumerate()
                    .map(|(i, id)| (id, i))
                    .collect();

                // For every edge (from -> to), from must come before to
                for edge in &workflow.edges {
                    let from_pos = positions.get(&edge.from).unwrap();
                    let to_pos = positions.get(&edge.to).unwrap();
                    prop_assert!(
                        from_pos < to_pos,
                        "Edge {:?} -> {:?} violates topological order (positions {} >= {})",
                        edge.from,
                        edge.to,
                        from_pos,
                        to_pos
                    );
                }
            }

            /// Property: Execution levels should be valid
            #[test]
            fn prop_execution_levels_valid(workflow in linear_workflow_strategy()) {
                let engine = Engine::new();
                let levels = engine.compute_execution_levels(&workflow);

                // Must succeed for valid workflows
                prop_assert!(levels.is_ok());
                let levels = levels.unwrap();

                // Level 0 should contain at least one node
                prop_assert!(!levels[0].is_empty());

                // All nodes must be assigned to some level
                let total_nodes: usize = levels.iter().map(|l| l.len()).sum();
                prop_assert_eq!(total_nodes, workflow.nodes.len());

                // No node should appear in multiple levels
                let mut all_nodes: Vec<&NodeId> = vec![];
                for level in &levels {
                    all_nodes.extend(level.iter());
                }
                all_nodes.sort();
                all_nodes.dedup();
                prop_assert_eq!(all_nodes.len(), workflow.nodes.len());
            }

            /// Property: Workflow hash should be consistent
            #[test]
            fn prop_workflow_hash_consistency(workflow in linear_workflow_strategy()) {
                let hash1 = plan_cache::hash_workflow_structure(&workflow.nodes, &workflow.edges);
                let hash2 = plan_cache::hash_workflow_structure(&workflow.nodes, &workflow.edges);
                prop_assert_eq!(hash1, hash2, "Workflow hash must be deterministic");
            }

            /// Property: Execution stats should be valid
            #[test]
            fn prop_execution_stats_valid(_workflow in linear_workflow_strategy()) {
                let engine = Engine::new();
                let stats = engine.get_stats();

                // Success rate should be in valid range (0.0 - 100.0)
                prop_assert!(stats.workflow_success_rate >= 0.0);
                prop_assert!(stats.workflow_success_rate <= 100.0);

                // Successful + failed should equal total
                prop_assert_eq!(
                    stats.successful_workflows + stats.failed_workflows,
                    stats.total_workflows
                );
            }
        }
    }
}
