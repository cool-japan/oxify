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
mod form_store;
mod graceful_degradation;
mod health_checker;
mod llm_cache;
mod loop_executor;
mod mcp_executor;
mod metrics;
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
pub use otel::{init_tracing, shutdown_tracing, TracingConfig};
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

    /// Build the Engine instance
    pub fn build(self) -> Engine {
        Engine {
            approval_store: self.approval_store,
            form_store: self.form_store,
            checkpoint_store: self.checkpoint_store,
            event_bus: self.event_bus,
            metrics: self.metrics,
            pause_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
            cancel_flag: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }
}

/// DAG execution engine
pub struct Engine {
    /// Approval store for human-in-the-loop workflows
    approval_store: Option<ApprovalStore>,

    /// Form store for human-in-the-loop workflows
    form_store: Option<FormStore>,

    /// Checkpoint store for pause/resume and recovery
    checkpoint_store: Option<Arc<dyn CheckpointStore>>,

    /// Event bus for publishing execution events
    event_bus: Option<Arc<EventBus>>,

    /// Metrics collector
    metrics: Option<Arc<ExecutionMetrics>>,

    /// Pause control flag
    pause_flag: Arc<std::sync::RwLock<HashMap<uuid::Uuid, bool>>>,

    /// Cancel control flag
    cancel_flag: Arc<std::sync::RwLock<HashMap<uuid::Uuid, bool>>>,
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
    async fn emit_event(&self, event: WorkflowEvent) {
        if let Some(event_bus) = &self.event_bus {
            let _ = event_bus.publish(event).await;
        }
    }

    /// Pause an execution
    pub fn pause_execution(&self, execution_id: uuid::Uuid) {
        self.pause_flag.write().unwrap().insert(execution_id, true);
        tracing::info!("Paused execution {}", execution_id);
    }

    /// Resume an execution
    pub fn resume_execution(&self, execution_id: uuid::Uuid) {
        self.pause_flag.write().unwrap().insert(execution_id, false);
        tracing::info!("Resumed execution {}", execution_id);
    }

    /// Check if an execution is paused
    pub fn is_paused(&self, execution_id: uuid::Uuid) -> bool {
        self.pause_flag
            .read()
            .unwrap()
            .get(&execution_id)
            .copied()
            .unwrap_or(false)
    }

    /// Cancel an execution
    pub fn cancel_execution(&self, execution_id: uuid::Uuid) {
        self.cancel_flag.write().unwrap().insert(execution_id, true);
        tracing::warn!("Cancelled execution {}", execution_id);
    }

    /// Check if an execution is cancelled
    pub fn is_cancelled(&self, execution_id: uuid::Uuid) -> bool {
        self.cancel_flag
            .read()
            .unwrap()
            .get(&execution_id)
            .copied()
            .unwrap_or(false)
    }

    /// Clear cancellation flag for an execution
    pub fn clear_cancellation(&self, execution_id: uuid::Uuid) {
        self.cancel_flag.write().unwrap().remove(&execution_id);
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

    /// Execute workflow from a checkpoint
    async fn execute_from_checkpoint(
        &self,
        workflow: &Workflow,
        checkpoint: ExecutionCheckpoint,
    ) -> Result<ExecutionContext> {
        let mut ctx = checkpoint.context;
        let completed_nodes = checkpoint.completed_nodes;

        // Get execution levels
        let execution_levels = self.compute_execution_levels(workflow)?;

        // Skip already completed levels and nodes
        for (level_idx, level_nodes) in execution_levels.iter().enumerate() {
            // Skip levels before the checkpoint
            if level_idx < checkpoint.current_level {
                continue;
            }

            for node_id in level_nodes {
                // Skip completed nodes
                if completed_nodes.contains(node_id) {
                    continue;
                }

                // Check if paused
                if self.is_paused(checkpoint.execution_id) {
                    tracing::info!(
                        "Execution {} paused, creating checkpoint",
                        checkpoint.execution_id
                    );
                    let _ = self.create_checkpoint(
                        workflow.metadata.id,
                        checkpoint.execution_id,
                        &ctx,
                        completed_nodes.clone(),
                        level_idx,
                        "paused".to_string(),
                    );
                    return Err(EngineError::ExecutionError("Execution paused".to_string()));
                }

                let node = workflow
                    .get_node(node_id)
                    .ok_or(EngineError::NodeNotFound(*node_id))?;

                let node_result = self.execute_node_with_retry(node, &ctx, workflow).await?;

                ctx.node_results.insert(*node_id, node_result);
            }
        }

        Ok(ctx)
    }

    /// Execute a workflow sequentially (without spawning tasks)
    /// Used for sub-workflows to avoid nested tokio::spawn issues
    pub async fn execute_sequential(&self, workflow: &Workflow) -> Result<ExecutionContext> {
        // Validate workflow first
        workflow.validate().map_err(EngineError::ValidationError)?;

        let mut ctx = ExecutionContext::new(workflow.metadata.id);

        // Get execution levels
        let execution_levels = self.compute_execution_levels(workflow)?;

        // Execute each level sequentially (no spawning)
        for level_nodes in execution_levels {
            for node_id in &level_nodes {
                let node = workflow
                    .get_node(node_id)
                    .ok_or(EngineError::NodeNotFound(*node_id))?;

                let node_result = self.execute_node_with_retry(node, &ctx, workflow).await?;

                ctx.record_node_result(*node_id, node_result);
            }
        }

        // Mark as completed
        ctx.state = ExecutionState::Completed;
        ctx.mark_completed();

        Ok(ctx)
    }

    /// Execute a workflow with parallel node execution
    pub async fn execute(&self, workflow: &Workflow) -> Result<ExecutionContext> {
        self.execute_with_config(workflow, ExecutionConfig::default())
            .await
    }

    /// Execute a workflow with custom configuration
    ///
    /// This method supports:
    /// - Automatic checkpointing after levels
    /// - Event emission for monitoring
    /// - Per-node timeouts
    /// - Concurrency limits
    pub async fn execute_with_config(
        &self,
        workflow: &Workflow,
        config: ExecutionConfig,
    ) -> Result<ExecutionContext> {
        // Validate workflow first
        workflow.validate().map_err(EngineError::ValidationError)?;

        let mut ctx = ExecutionContext::new(workflow.metadata.id);
        let execution_id = ctx.execution_id;
        let workflow_id = workflow.metadata.id;
        let start_time = std::time::Instant::now();

        // Get execution levels for parallel execution
        let execution_levels = self.compute_execution_levels(workflow)?;
        let total_levels = execution_levels.len();
        let total_nodes = workflow.nodes.len();

        // Create backpressure monitor if configured
        let backpressure_monitor = config
            .backpressure_config
            .as_ref()
            .map(|bp_config| Arc::new(BackpressureMonitor::new(bp_config.clone())));

        // Create resource monitor if configured
        let resource_monitor = config
            .resource_config
            .as_ref()
            .map(|res_config| Arc::new(ResourceMonitor::new(res_config.clone())));

        // Create resource enforcer if configured
        let resource_enforcer = config
            .resource_limits
            .as_ref()
            .map(|limits| Arc::new(ResourceEnforcer::new(limits.clone())));

        // Check initial resource limits
        if let Some(ref enforcer) = resource_enforcer {
            if let Err(e) = enforcer.check_memory() {
                return Err(EngineError::ExecutionError(e.to_string()));
            }
            if let Err(e) = enforcer.check_execution_time() {
                return Err(EngineError::ExecutionError(e.to_string()));
            }
        }

        // Emit workflow started event
        if config.emit_events {
            self.emit_event(WorkflowEvent::workflow_started(workflow_id, execution_id))
                .await;
        }

        let mut completed_nodes_count = 0;
        let mut completed_node_ids: Vec<NodeId> = Vec::new();

        // Execute each level in parallel
        for (level_idx, level_nodes) in execution_levels.iter().enumerate() {
            // Check if cancelled
            if self.is_cancelled(execution_id) {
                if config.emit_events {
                    self.emit_event(WorkflowEvent::workflow_failed(
                        workflow_id,
                        execution_id,
                        "execution_cancelled",
                    ))
                    .await;
                }

                // Clear cancellation flag
                self.clear_cancellation(execution_id);

                return Err(EngineError::ExecutionCancelled);
            }

            // Check if paused
            if self.is_paused(execution_id) {
                if config.emit_events {
                    self.emit_event(WorkflowEvent::workflow_paused(
                        workflow_id,
                        execution_id,
                        "user_requested",
                    ))
                    .await;
                }

                // Create checkpoint before returning
                if self.checkpoint_store.is_some() {
                    let checkpoint_id = self.create_checkpoint(
                        workflow_id,
                        execution_id,
                        &ctx,
                        completed_node_ids.clone(),
                        level_idx,
                        "paused".to_string(),
                    )?;

                    if config.emit_events {
                        self.emit_event(WorkflowEvent::checkpoint_created(
                            workflow_id,
                            execution_id,
                            checkpoint_id,
                            level_idx,
                        ))
                        .await;
                    }
                }

                return Err(EngineError::ExecutionPaused);
            }

            // Emit level started event
            if config.emit_events {
                self.emit_event(WorkflowEvent::level_started(
                    workflow_id,
                    execution_id,
                    level_idx,
                    level_nodes.len(),
                ))
                .await;
            }

            // Spawn tasks for all nodes at this level
            let mut handles = Vec::new();

            // Determine concurrency limit
            let max_concurrent = if let Some(ref monitor) = resource_monitor {
                // Use resource-aware concurrency
                monitor.recommended_concurrency().min(level_nodes.len())
            } else {
                // Use static concurrency limit
                config
                    .max_concurrent_nodes
                    .unwrap_or(level_nodes.len())
                    .min(level_nodes.len())
            };

            // Process nodes in batches based on concurrency limit
            for chunk in level_nodes.chunks(max_concurrent) {
                for node_id in chunk {
                    let node = workflow
                        .get_node(node_id)
                        .ok_or(EngineError::NodeNotFound(*node_id))?
                        .clone();

                    // Apply backpressure if configured
                    if let Some(ref monitor) = backpressure_monitor {
                        use crate::backpressure::BackpressureStrategy;

                        match monitor.strategy() {
                            BackpressureStrategy::Block => {
                                // Block until queue space is available
                                while monitor.should_apply_backpressure() {
                                    monitor.record_blocked();
                                    tokio::time::sleep(tokio::time::Duration::from_millis(10))
                                        .await;
                                }
                            }
                            BackpressureStrategy::Drop => {
                                // Drop task if backpressure is active
                                if monitor.should_apply_backpressure() {
                                    monitor.record_dropped();
                                    continue; // Skip this node
                                }
                            }
                            BackpressureStrategy::Throttle => {
                                // Throttle by adding delay if backpressure is active
                                if monitor.should_apply_backpressure() {
                                    monitor.record_throttled();
                                    tokio::time::sleep(monitor.throttle_delay()).await;
                                }
                            }
                            BackpressureStrategy::None => {
                                // No backpressure, proceed normally
                            }
                        }

                        // Record node as queued
                        monitor.record_queued();
                    }

                    // Check resource limits before executing node
                    if let Some(ref enforcer) = resource_enforcer {
                        // Check if we've exceeded the total node execution limit
                        enforcer
                            .check_node_limit()
                            .map_err(|e| EngineError::ExecutionError(e.to_string()))?;

                        // Check memory limits
                        if let Err(e) = enforcer.check_memory() {
                            return Err(EngineError::ExecutionError(e.to_string()));
                        }

                        // Check execution time limit
                        if let Err(e) = enforcer.check_execution_time() {
                            return Err(EngineError::ExecutionError(e.to_string()));
                        }
                    }

                    let ctx_clone = ctx.clone();
                    let workflow_clone = workflow.clone();
                    let node_timeout_ms = config.node_timeout_ms;
                    let emit_events = config.emit_events;
                    let event_bus = self.event_bus.clone();
                    let bp_monitor = backpressure_monitor.clone();
                    let res_enforcer = resource_enforcer.clone();

                    // Spawn concurrent execution for this node with retry support
                    let handle = tokio::spawn(async move {
                        // Record node dequeued (starting execution)
                        if let Some(ref monitor) = bp_monitor {
                            monitor.record_dequeued();
                        }

                        let node_start = std::time::Instant::now();

                        // Emit node started event
                        if emit_events {
                            if let Some(bus) = &event_bus {
                                let _ = bus
                                    .publish(WorkflowEvent::node_started(
                                        workflow_id,
                                        execution_id,
                                        node.id,
                                        &node.name,
                                    ))
                                    .await;
                            }
                        }

                        let engine = Engine::new();

                        // Execute with optional timeout
                        let result = if let Some(timeout_ms) = node_timeout_ms {
                            match tokio::time::timeout(
                                tokio::time::Duration::from_millis(timeout_ms),
                                engine.execute_node_with_retry(&node, &ctx_clone, &workflow_clone),
                            )
                            .await
                            {
                                Ok(result) => result,
                                Err(_) => Err(EngineError::Timeout(format!(
                                    "Node '{}' timed out after {}ms",
                                    node.name, timeout_ms
                                ))),
                            }
                        } else {
                            engine
                                .execute_node_with_retry(&node, &ctx_clone, &workflow_clone)
                                .await
                        };

                        let duration_ms = node_start.elapsed().as_millis();

                        // Emit node completed/failed event
                        if emit_events {
                            if let Some(bus) = &event_bus {
                                match &result {
                                    Ok(_) => {
                                        let _ = bus
                                            .publish(WorkflowEvent::node_completed(
                                                workflow_id,
                                                execution_id,
                                                node.id,
                                                &node.name,
                                                duration_ms,
                                            ))
                                            .await;
                                    }
                                    Err(e) => {
                                        let _ = bus
                                            .publish(WorkflowEvent::node_failed(
                                                workflow_id,
                                                execution_id,
                                                node.id,
                                                &node.name,
                                                &e.to_string(),
                                            ))
                                            .await;
                                    }
                                }
                            }
                        }

                        // Record node completed
                        if let Some(ref monitor) = bp_monitor {
                            monitor.record_completed();
                        }

                        // Track resource usage
                        if let Some(ref enforcer) = res_enforcer {
                            enforcer.usage().add_node_execution();
                        }

                        (node.id, result)
                    });

                    handles.push(handle);
                }

                // Wait for all nodes in this batch to complete
                for handle in handles.drain(..) {
                    let (node_id, result) = handle.await.map_err(|e| {
                        EngineError::ExecutionError(format!("Task join error: {}", e))
                    })?;

                    match result {
                        Ok(node_result) => {
                            ctx.record_node_result(node_id, node_result);
                            completed_nodes_count += 1;
                            completed_node_ids.push(node_id);
                        }
                        Err(e) if config.continue_on_error => {
                            // Record failure but continue
                            let mut failed_result = NodeExecutionResult::new();
                            failed_result =
                                failed_result.complete(ExecutionResult::Failure(e.to_string()));
                            ctx.record_node_result(node_id, failed_result);
                            completed_nodes_count += 1;
                            completed_node_ids.push(node_id);
                        }
                        Err(e) => {
                            // Emit workflow failed event
                            if config.emit_events {
                                self.emit_event(WorkflowEvent::workflow_failed(
                                    workflow_id,
                                    execution_id,
                                    &e.to_string(),
                                ))
                                .await;
                            }
                            return Err(e);
                        }
                    }
                }
            }

            // Emit level completed event
            if config.emit_events {
                self.emit_event(WorkflowEvent::level_completed(
                    workflow_id,
                    execution_id,
                    level_idx,
                ))
                .await;

                // Emit progress update
                self.emit_event(WorkflowEvent::progress_update(
                    workflow_id,
                    execution_id,
                    completed_nodes_count,
                    total_nodes,
                    level_idx + 1,
                    total_levels,
                ))
                .await;
            }

            // Create checkpoint based on frequency
            let should_checkpoint = match config.checkpoint_frequency {
                CheckpointFrequency::Never => false,
                CheckpointFrequency::EveryLevel => true,
                CheckpointFrequency::EveryNLevels(n) => (level_idx + 1) % n == 0,
                CheckpointFrequency::EveryNNodes(n) => completed_nodes_count % n == 0,
            };

            if should_checkpoint && self.checkpoint_store.is_some() {
                let checkpoint_id = self.create_checkpoint(
                    workflow_id,
                    execution_id,
                    &ctx,
                    completed_node_ids.clone(),
                    level_idx + 1,
                    "automatic".to_string(),
                )?;

                if config.emit_events {
                    self.emit_event(WorkflowEvent::checkpoint_created(
                        workflow_id,
                        execution_id,
                        checkpoint_id,
                        level_idx + 1,
                    ))
                    .await;
                }
            }
        }

        // Mark as completed
        ctx.state = ExecutionState::Completed;
        ctx.mark_completed();

        let duration_ms = start_time.elapsed().as_millis();

        // Emit workflow completed event
        if config.emit_events {
            self.emit_event(WorkflowEvent::workflow_completed(
                workflow_id,
                execution_id,
                completed_nodes_count,
                duration_ms,
            ))
            .await;
        }

        Ok(ctx)
    }

    /// Execute a node with retry logic
    async fn execute_node_with_retry(
        &self,
        node: &Node,
        ctx: &ExecutionContext,
        workflow: &Workflow,
    ) -> Result<NodeExecutionResult> {
        let retry_config = node.retry_config.as_ref();
        let max_retries = retry_config.map(|c| c.max_retries).unwrap_or(0);

        let mut attempt = 0;

        loop {
            // Execute the node
            match self.execute_node(node, ctx, workflow).await {
                Ok(mut result) => {
                    result.retry_count = attempt;
                    return Ok(result);
                }
                Err(e) => {
                    attempt += 1;

                    // Check if we should retry
                    if attempt > max_retries {
                        // Max retries exceeded
                        let mut result = NodeExecutionResult::new();
                        result.retry_count = attempt - 1;
                        result = result.complete(ExecutionResult::Failure(format!(
                            "Failed after {} retries: {}",
                            max_retries, e
                        )));
                        return Ok(result);
                    }

                    // Calculate delay with exponential backoff
                    if let Some(config) = retry_config {
                        let delay_ms = (config.initial_delay_ms as f64
                            * config.backoff_multiplier.powi((attempt - 1) as i32))
                            as u64;
                        let delay_ms = delay_ms.min(config.max_delay_ms);

                        tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }
    }

    /// Execute a single node
    async fn execute_node(
        &self,
        node: &Node,
        ctx: &ExecutionContext,
        _workflow: &Workflow,
    ) -> Result<NodeExecutionResult> {
        let node_result = NodeExecutionResult::new();

        let result = match &node.kind {
            NodeKind::Start => {
                // Start node just passes through
                ExecutionResult::Success(serde_json::json!({}))
            }
            NodeKind::End => {
                // End node just passes through
                ExecutionResult::Success(serde_json::json!({}))
            }
            NodeKind::LLM(config) => {
                // Resolve template variables in prompt
                let prompt = self.resolve_template(&config.prompt_template, ctx)?;

                // Create appropriate LLM provider based on configuration
                let provider_name = config.provider.to_lowercase();

                // Try to execute with real LLM provider
                match provider_name.as_str() {
                    "openai" => {
                        // Parse tools and images from configuration
                        let tools: Vec<Tool> = config
                            .tools
                            .iter()
                            .filter_map(|t| serde_json::from_value(t.clone()).ok())
                            .collect();
                        let images: Vec<ImageInput> = config
                            .images
                            .iter()
                            .filter_map(|i| serde_json::from_value(i.clone()).ok())
                            .collect();

                        let llm_request = LlmRequest {
                            prompt: prompt.clone(),
                            system_prompt: config.system_prompt.clone(),
                            temperature: config.temperature,
                            max_tokens: config.max_tokens,
                            tools,
                            images,
                        };

                        // Check cache first
                        if let Some(cached_response) =
                            LLM_CACHE.get("openai", &config.model, &llm_request)
                        {
                            ExecutionResult::Success(serde_json::json!({
                                "provider": "openai",
                                "model": config.model,
                                "prompt": prompt,
                                "response": cached_response.content,
                                "usage": cached_response.usage,
                                "cached": true
                            }))
                        } else {
                            // Cache miss - call API
                            let api_key = std::env::var("OPENAI_API_KEY")
                                .unwrap_or_else(|_| "missing-api-key".to_string());

                            let provider = OpenAIProvider::new(api_key, config.model.clone());

                            match provider.complete(llm_request.clone()).await {
                                Ok(response) => {
                                    // Store in cache
                                    LLM_CACHE.put(
                                        "openai",
                                        &config.model,
                                        &llm_request,
                                        response.clone(),
                                    );

                                    ExecutionResult::Success(serde_json::json!({
                                        "provider": "openai",
                                        "model": config.model,
                                        "prompt": prompt,
                                        "response": response.content,
                                        "usage": response.usage,
                                        "cached": false
                                    }))
                                }
                                Err(e) => {
                                    ExecutionResult::Failure(format!("OpenAI API error: {}", e))
                                }
                            }
                        }
                    }
                    "anthropic" | "claude" => {
                        // Parse tools and images from configuration
                        let tools: Vec<Tool> = config
                            .tools
                            .iter()
                            .filter_map(|t| serde_json::from_value(t.clone()).ok())
                            .collect();
                        let images: Vec<ImageInput> = config
                            .images
                            .iter()
                            .filter_map(|i| serde_json::from_value(i.clone()).ok())
                            .collect();

                        let llm_request = LlmRequest {
                            prompt: prompt.clone(),
                            system_prompt: config.system_prompt.clone(),
                            temperature: config.temperature,
                            max_tokens: config.max_tokens,
                            tools,
                            images,
                        };

                        // Check cache first
                        if let Some(cached_response) =
                            LLM_CACHE.get("anthropic", &config.model, &llm_request)
                        {
                            ExecutionResult::Success(serde_json::json!({
                                "provider": "anthropic",
                                "model": config.model,
                                "prompt": prompt,
                                "response": cached_response.content,
                                "usage": cached_response.usage,
                                "cached": true
                            }))
                        } else {
                            // Cache miss - call API
                            let api_key = std::env::var("ANTHROPIC_API_KEY")
                                .unwrap_or_else(|_| "missing-api-key".to_string());

                            let provider = AnthropicProvider::new(api_key, config.model.clone());

                            match provider.complete(llm_request.clone()).await {
                                Ok(response) => {
                                    // Store in cache
                                    LLM_CACHE.put(
                                        "anthropic",
                                        &config.model,
                                        &llm_request,
                                        response.clone(),
                                    );

                                    ExecutionResult::Success(serde_json::json!({
                                        "provider": "anthropic",
                                        "model": config.model,
                                        "prompt": prompt,
                                        "response": response.content,
                                        "usage": response.usage,
                                        "cached": false
                                    }))
                                }
                                Err(e) => {
                                    ExecutionResult::Failure(format!("Anthropic API error: {}", e))
                                }
                            }
                        }
                    }
                    "ollama" | "local" => {
                        // Parse tools and images from configuration
                        let tools: Vec<Tool> = config
                            .tools
                            .iter()
                            .filter_map(|t| serde_json::from_value(t.clone()).ok())
                            .collect();
                        let images: Vec<ImageInput> = config
                            .images
                            .iter()
                            .filter_map(|i| serde_json::from_value(i.clone()).ok())
                            .collect();

                        let llm_request = LlmRequest {
                            prompt: prompt.clone(),
                            system_prompt: config.system_prompt.clone(),
                            temperature: config.temperature,
                            max_tokens: config.max_tokens,
                            tools,
                            images,
                        };

                        // Check cache first
                        if let Some(cached_response) =
                            LLM_CACHE.get("ollama", &config.model, &llm_request)
                        {
                            ExecutionResult::Success(serde_json::json!({
                                "provider": "ollama",
                                "model": config.model,
                                "prompt": prompt,
                                "response": cached_response.content,
                                "usage": cached_response.usage,
                                "cached": true
                            }))
                        } else {
                            // Cache miss - call API
                            let provider = OllamaProvider::new(config.model.clone());

                            match provider.complete(llm_request.clone()).await {
                                Ok(response) => {
                                    // Store in cache
                                    LLM_CACHE.put(
                                        "ollama",
                                        &config.model,
                                        &llm_request,
                                        response.clone(),
                                    );

                                    ExecutionResult::Success(serde_json::json!({
                                        "provider": "ollama",
                                        "model": config.model,
                                        "prompt": prompt,
                                        "response": response.content,
                                        "usage": response.usage,
                                        "cached": false
                                    }))
                                }
                                Err(e) => {
                                    ExecutionResult::Failure(format!("Ollama API error: {}", e))
                                }
                            }
                        }
                    }
                    _ => {
                        // Unknown provider, return placeholder
                        ExecutionResult::Success(serde_json::json!({
                            "provider": config.provider,
                            "model": config.model,
                            "prompt": prompt,
                            "response": "[Unsupported LLM provider placeholder]",
                            "warning": format!("Provider '{}' not implemented yet", config.provider)
                        }))
                    }
                }
            }
            NodeKind::Retriever(config) => {
                // Resolve query template
                let query_text = self.resolve_template(&config.query, ctx)?;

                // Generate embedding for query using OpenAI
                let openai_key = std::env::var("OPENAI_API_KEY")
                    .unwrap_or_else(|_| "missing-api-key".to_string());

                let embedding_provider = OpenAIProvider::for_embeddings(openai_key);

                // Generate query embedding
                let embedding_result = embedding_provider
                    .embed(EmbeddingRequest {
                        texts: vec![query_text.clone()],
                        model: None,
                    })
                    .await;

                match embedding_result {
                    Ok(embedding_response) => {
                        let query_vector = &embedding_response.embeddings[0];

                        // Execute search based on db_type
                        self.execute_vector_search(
                            &config.db_type,
                            &config.collection,
                            query_vector,
                            config.top_k,
                            config.score_threshold,
                            &query_text,
                        )
                        .await
                    }
                    Err(e) => {
                        ExecutionResult::Failure(format!("Embedding generation error: {}", e))
                    }
                }
            }
            NodeKind::Code(config) => {
                // Execute code using the code executor
                let executor = CodeExecutor::new();

                match executor.execute(config, ctx).await {
                    Ok(result) => ExecutionResult::Success(serde_json::json!({
                        "runtime": config.runtime,
                        "output": config.output,
                        "result": result
                    })),
                    Err(e) => ExecutionResult::Failure(format!("Code execution error: {}", e)),
                }
            }
            NodeKind::IfElse(condition) => {
                // Evaluate condition using expression evaluator
                let evaluator = ConditionalEvaluator::new(ctx).map_err(|e| {
                    EngineError::ExecutionError(format!("Failed to create evaluator: {}", e))
                })?;

                let condition_met = evaluator.evaluate(&condition.expression).map_err(|e| {
                    EngineError::ExecutionError(format!("Condition evaluation failed: {}", e))
                })?;

                let next_node = if condition_met {
                    condition.true_branch
                } else {
                    condition.false_branch
                };

                ExecutionResult::Success(serde_json::json!({
                    "expression": condition.expression,
                    "condition_met": condition_met,
                    "next_branch": if condition_met { "true" } else { "false" },
                    "next_node": next_node.to_string()
                }))
            }
            NodeKind::Tool(config) => {
                // MCP tool execution with HTTP fallback
                // For MCP servers: server_id = "https://mcp.example.com", tool_name = "tool_name"
                // For HTTP requests: server_id = "https://api.example.com", tool_name = "GET /path"
                MCP_EXECUTOR
                    .execute_tool(
                        &config.server_id,
                        &config.tool_name,
                        config.parameters.clone(),
                    )
                    .await
            }
            NodeKind::Loop(config) => {
                // Execute loop
                match LoopExecutor::execute(config, ctx).await {
                    Ok(results) => ExecutionResult::Success(serde_json::json!({
                        "loop_type": match &config.loop_type {
                            oxify_model::LoopType::ForEach { .. } => "foreach",
                            oxify_model::LoopType::While { .. } => "while",
                            oxify_model::LoopType::Repeat { .. } => "repeat",
                        },
                        "iterations": results.len(),
                        "results": results,
                    })),
                    Err(e) => ExecutionResult::Failure(format!("Loop execution error: {}", e)),
                }
            }
            NodeKind::TryCatch(config) => {
                // Execute try-catch-finally
                match TryCatchExecutor::execute(config, ctx).await {
                    Ok(result) => {
                        if result.succeeded {
                            ExecutionResult::Success(serde_json::json!({
                                "try_result": result.try_result,
                                "catch_result": result.catch_result,
                                "finally_result": result.finally_result,
                                "error_handled": result.error.is_some(),
                            }))
                        } else {
                            ExecutionResult::Failure(
                                result.error.unwrap_or_else(|| "Unknown error".to_string()),
                            )
                        }
                    }
                    Err(e) => ExecutionResult::Failure(format!("Try-catch execution error: {}", e)),
                }
            }
            NodeKind::SubWorkflow(config) => {
                // Execute sub-workflow
                match SubWorkflowExecutor::execute(config, ctx).await {
                    Ok(result) => ExecutionResult::Success(serde_json::json!({
                        "sub_workflow_id": result.sub_workflow_id,
                        "execution_state": format!("{:?}", result.execution_state),
                        "node_count": result.node_count,
                        "output": result.output,
                    })),
                    Err(e) => {
                        ExecutionResult::Failure(format!("Sub-workflow execution error: {}", e))
                    }
                }
            }
            NodeKind::Switch(config) => {
                // Resolve the switch_on expression
                let switch_value = self.resolve_template(&config.switch_on, ctx)?;

                // Try to match against each case
                for case in &config.cases {
                    let match_result = if case.match_value.starts_with("regex:") {
                        // Regex matching
                        let pattern = &case.match_value[6..]; // Remove "regex:" prefix
                        if let Ok(re) = regex::Regex::new(pattern) {
                            re.is_match(&switch_value)
                        } else {
                            false
                        }
                    } else {
                        // Exact match
                        switch_value == case.match_value
                    };

                    if match_result {
                        // Execute the matched action
                        let result = self.resolve_template(&case.action, ctx)?;
                        return Ok(node_result.complete(ExecutionResult::Success(
                            serde_json::json!({
                                "matched_case": case.match_value.clone(),
                                "result": result,
                            }),
                        )));
                    }
                }

                // No match found - try default case
                if let Some(default_action) = &config.default_case {
                    let result = self.resolve_template(default_action, ctx)?;
                    ExecutionResult::Success(serde_json::json!({
                        "matched_case": "default",
                        "result": result,
                    }))
                } else {
                    ExecutionResult::Failure(format!(
                        "No matching case for value: {}",
                        switch_value
                    ))
                }
            }
            NodeKind::Parallel(config) => {
                use futures::stream::{FuturesUnordered, StreamExt};
                use tokio::time::{timeout, Duration};

                // Prepare tasks
                let mut task_futures = FuturesUnordered::new();

                // Execute tasks with concurrency control
                let max_concurrent = config.max_concurrency.unwrap_or(config.tasks.len());
                let mut pending_tasks: Vec<&ParallelTask> = config.tasks.iter().collect();
                let mut active_tasks = 0;
                let mut results = HashMap::new();

                while !pending_tasks.is_empty() || active_tasks > 0 {
                    // Spawn new tasks up to max concurrency
                    while active_tasks < max_concurrent && !pending_tasks.is_empty() {
                        let task = pending_tasks.remove(0);
                        let task_id = task.id.clone();
                        let expression = task.expression.clone();
                        let ctx_clone = ctx.clone();

                        let fut = async move {
                            let engine = Engine::new();
                            let resolved = engine.resolve_template(&expression, &ctx_clone)?;
                            Ok::<(String, String), EngineError>((task_id, resolved))
                        };

                        task_futures.push(fut);
                        active_tasks += 1;
                    }

                    // Wait for next task to complete
                    if let Some(result) = task_futures.next().await {
                        match result {
                            Ok((task_id, value)) => {
                                results.insert(
                                    task_id,
                                    serde_json::json!({"status": "success", "value": value}),
                                );
                            }
                            Err(e) => {
                                // Handle failure based on strategy
                                if config.strategy == ParallelStrategy::WaitAll {
                                    return Ok(node_result.complete(ExecutionResult::Failure(
                                        format!("Parallel task failed: {}", e),
                                    )));
                                }
                                // For Race and AllSettled, continue
                                results.insert(
                                    "failed_task".to_string(),
                                    serde_json::json!({"status": "error", "error": e.to_string()}),
                                );
                            }
                        }
                        active_tasks -= 1;

                        // For Race strategy, return immediately on first success
                        if config.strategy == ParallelStrategy::Race && !results.is_empty() {
                            break;
                        }
                    }
                }

                // Apply timeout if specified
                let final_result = if let Some(timeout_ms) = config.timeout_ms {
                    let timeout_duration = Duration::from_millis(timeout_ms);
                    match timeout(timeout_duration, async {
                        // Already completed above
                        Ok::<_, EngineError>(())
                    })
                    .await
                    {
                        Ok(_) => ExecutionResult::Success(serde_json::json!({
                            "strategy": format!("{:?}", config.strategy),
                            "results": results,
                        })),
                        Err(_) => {
                            ExecutionResult::Failure("Parallel execution timeout".to_string())
                        }
                    }
                } else {
                    ExecutionResult::Success(serde_json::json!({
                        "strategy": format!("{:?}", config.strategy),
                        "results": results,
                    }))
                };

                final_result
            }
            NodeKind::Approval(config) => {
                // Check if approval store is available
                let store = match &self.approval_store {
                    Some(store) => store,
                    None => {
                        return Ok(node_result.complete(ExecutionResult::Failure(
                            "Approval store not configured. Create engine with `Engine::with_approval_store()`.".to_string()
                        )));
                    }
                };

                // Create approval request
                let execution_id = ctx.execution_id.to_string();
                let approval_request =
                    ApprovalRequest::new(execution_id.clone(), node.id, config.clone(), ctx);

                let approval_id = store.add(approval_request);

                tracing::info!(
                    "Approval request created: {} for node {} in execution {}",
                    approval_id,
                    node.id,
                    execution_id
                );

                // Poll for approval with timeout
                let poll_interval = std::time::Duration::from_secs(1);
                let max_wait = config.timeout_seconds.unwrap_or(3600); // Default 1 hour
                let start_time = std::time::Instant::now();

                loop {
                    // Get current approval status
                    let approval = store.get(approval_id).expect("Approval should exist");

                    match approval.status {
                        ApprovalStatus::Approved => {
                            tracing::info!(
                                "Approval {} approved by {:?}",
                                approval_id,
                                approval.resolved_by
                            );
                            break ExecutionResult::Success(serde_json::json!({
                                "approval_id": approval_id.to_string(),
                                "status": "approved",
                                "approved_by": approval.resolved_by,
                                "comments": approval.comments,
                            }));
                        }
                        ApprovalStatus::Rejected => {
                            tracing::info!(
                                "Approval {} rejected by {:?}",
                                approval_id,
                                approval.resolved_by
                            );
                            break ExecutionResult::Failure(format!(
                                "Approval rejected by {:?}: {}",
                                approval.resolved_by,
                                approval.comments.unwrap_or_default()
                            ));
                        }
                        ApprovalStatus::Pending => {
                            // Check for timeout
                            if start_time.elapsed().as_secs() >= max_wait {
                                // Mark as timed out
                                if let Some(mut approval) = store.get(approval_id) {
                                    approval.mark_timed_out();
                                    store.update(approval);
                                }
                                tracing::warn!("Approval {} timed out", approval_id);
                                break ExecutionResult::Failure(format!(
                                    "Approval timed out after {} seconds",
                                    max_wait
                                ));
                            }

                            // Wait before next poll
                            tokio::time::sleep(poll_interval).await;
                        }
                        ApprovalStatus::TimedOut => {
                            break ExecutionResult::Failure("Approval timed out".to_string());
                        }
                    }
                }
            }
            NodeKind::Form(config) => {
                // Check if form store is available
                let store = match &self.form_store {
                    Some(store) => store,
                    None => {
                        return Ok(node_result.complete(ExecutionResult::Failure(
                            "Form store not configured. Create engine with `Engine::with_form_store()`.".to_string()
                        )));
                    }
                };

                // Create form submission request
                let execution_id = ctx.execution_id.to_string();
                let form_request =
                    FormSubmissionRequest::new(execution_id.clone(), node.id, config.clone(), ctx);

                let form_id = store.add(form_request);

                tracing::info!(
                    "Form submission request created: {} for node {} in execution {}",
                    form_id,
                    node.id,
                    execution_id
                );

                // Poll for form submission with timeout
                let poll_interval = std::time::Duration::from_secs(1);
                let max_wait = config.timeout_seconds.unwrap_or(3600); // Default 1 hour
                let start_time = std::time::Instant::now();

                loop {
                    // Get current form status
                    let form = store.get(form_id).expect("Form should exist");

                    match form.status {
                        FormStatus::Submitted => {
                            tracing::info!("Form {} submitted by {:?}", form_id, form.submitted_by);
                            break ExecutionResult::Success(serde_json::json!({
                                "form_id": form_id.to_string(),
                                "status": "submitted",
                                "submitted_by": form.submitted_by,
                                "form_data": form.form_data,
                            }));
                        }
                        FormStatus::Pending => {
                            // Check for timeout
                            if start_time.elapsed().as_secs() >= max_wait {
                                // Mark as timed out
                                if let Some(mut form) = store.get(form_id) {
                                    form.mark_timed_out();
                                    store.update(form);
                                }
                                tracing::warn!("Form {} timed out", form_id);
                                break ExecutionResult::Failure(format!(
                                    "Form submission timed out after {} seconds",
                                    max_wait
                                ));
                            }

                            // Wait before next poll
                            tokio::time::sleep(poll_interval).await;
                        }
                        FormStatus::TimedOut => {
                            break ExecutionResult::Failure(
                                "Form submission timed out".to_string(),
                            );
                        }
                    }
                }
            }

            NodeKind::Vision(config) => {
                // Get image data from context
                let image_input = self.resolve_template(&config.image_input, ctx)?;

                // Try to get image data - could be base64, file path, or raw bytes reference
                let image_data = if let Some(var_value) = ctx.get_variable(&image_input) {
                    // Variable contains image data
                    match var_value {
                        serde_json::Value::String(s) => {
                            // Could be base64 or file path
                            if s.starts_with("data:image") || s.len() > 1000 {
                                // Likely base64
                                use base64::Engine;
                                let b64_data = s.split(',').next_back().unwrap_or(s);
                                base64::engine::general_purpose::STANDARD
                                    .decode(b64_data)
                                    .unwrap_or_default()
                            } else {
                                // Try as file path
                                match std::fs::read(s) {
                                    Ok(data) => data,
                                    Err(e) => {
                                        return Ok(node_result.complete(ExecutionResult::Failure(
                                            format!("Failed to read image file '{}': {}", s, e),
                                        )));
                                    }
                                }
                            }
                        }
                        serde_json::Value::Array(arr) => {
                            // Array of bytes
                            arr.iter()
                                .filter_map(|v| v.as_u64().map(|n| n as u8))
                                .collect()
                        }
                        _ => {
                            return Ok(node_result.complete(ExecutionResult::Failure(
                                "Invalid image input type: expected string or byte array"
                                    .to_string(),
                            )));
                        }
                    }
                } else {
                    // Try as direct file path
                    match std::fs::read(&image_input) {
                        Ok(data) => data,
                        Err(e) => {
                            return Ok(node_result.complete(ExecutionResult::Failure(format!(
                                "Failed to read image '{}': {}",
                                image_input, e
                            ))));
                        }
                    }
                };

                // Create provider configuration
                let provider_config = match config.provider.to_lowercase().as_str() {
                    "mock" => VisionProviderConfig::mock(),
                    "tesseract" => VisionProviderConfig::tesseract(config.language.as_deref()),
                    "surya" => {
                        let model_path = match config.model_path.as_deref() {
                            Some(path) => path,
                            None => {
                                return Ok(node_result.complete(ExecutionResult::Failure(
                                    "Surya provider requires model_path configuration".to_string(),
                                )));
                            }
                        };
                        VisionProviderConfig::surya(model_path, config.use_gpu)
                    }
                    "paddle" => {
                        let model_path = match config.model_path.as_deref() {
                            Some(path) => path,
                            None => {
                                return Ok(node_result.complete(ExecutionResult::Failure(
                                    "PaddleOCR provider requires model_path configuration"
                                        .to_string(),
                                )));
                            }
                        };
                        VisionProviderConfig::paddle(model_path, config.use_gpu)
                    }
                    _ => {
                        return Ok(node_result.complete(ExecutionResult::Failure(format!(
                            "Unsupported vision provider: {}",
                            config.provider
                        ))));
                    }
                };

                // Create and initialize provider
                let vision_provider = match create_provider(&provider_config) {
                    Ok(provider) => provider,
                    Err(e) => {
                        return Ok(node_result.complete(ExecutionResult::Failure(format!(
                            "Failed to create vision provider: {}",
                            e
                        ))));
                    }
                };

                // Load model
                if let Err(e) = vision_provider.load_model().await {
                    return Ok(node_result.complete(ExecutionResult::Failure(format!(
                        "Failed to load vision model: {}",
                        e
                    ))));
                }

                // Process image
                let ocr_result = match vision_provider.process_image(&image_data).await {
                    Ok(result) => result,
                    Err(e) => {
                        return Ok(node_result.complete(ExecutionResult::Failure(format!(
                            "Vision processing failed: {}",
                            e
                        ))));
                    }
                };

                // Convert to JSON
                let result_json = match serde_json::to_value(&ocr_result) {
                    Ok(json) => json,
                    Err(e) => {
                        return Ok(node_result.complete(ExecutionResult::Failure(format!(
                            "Failed to serialize OCR result: {}",
                            e
                        ))));
                    }
                };

                ExecutionResult::Success(result_json)
            }
        };

        Ok(node_result.complete(result))
    }

    /// Check if all dependencies of a node are satisfied
    #[allow(dead_code)]
    fn dependencies_satisfied(
        &self,
        node_id: NodeId,
        workflow: &Workflow,
        ctx: &ExecutionContext,
    ) -> bool {
        // Get incoming edges
        let incoming = workflow.get_incoming_edges(&node_id);

        // All incoming nodes must have completed
        incoming
            .iter()
            .all(|edge| ctx.get_node_result(&edge.from).is_some())
    }

    /// Execute vector search across different database providers
    #[allow(clippy::too_many_arguments)]
    async fn execute_vector_search(
        &self,
        db_type: &str,
        collection: &str,
        query_vector: &[f32],
        top_k: usize,
        score_threshold: Option<f64>,
        query_text: &str,
    ) -> ExecutionResult {
        let db_type_lower = db_type.to_lowercase();

        match db_type_lower.as_str() {
            "qdrant" => {
                let url = std::env::var("QDRANT_URL")
                    .unwrap_or_else(|_| "http://localhost:6334".to_string());
                match QdrantProvider::new(&url).await {
                    Ok(provider) => {
                        self.search_with_provider(
                            provider,
                            "qdrant",
                            collection,
                            query_vector,
                            top_k,
                            score_threshold,
                            query_text,
                        )
                        .await
                    }
                    Err(e) => {
                        ExecutionResult::Failure(format!("Failed to connect to Qdrant: {}", e))
                    }
                }
            }
            // pgvector disabled - requires PostgreSQL
            "pgvector" | "postgres" | "postgresql" => ExecutionResult::Failure(
                "pgvector provider is disabled (requires PostgreSQL)".to_string(),
            ),
            "chromadb" | "chroma" => {
                let url = std::env::var("CHROMADB_URL")
                    .unwrap_or_else(|_| "http://localhost:8000".to_string());
                let provider = ChromaDBProvider::new(url);
                self.search_with_provider(
                    provider,
                    "chromadb",
                    collection,
                    query_vector,
                    top_k,
                    score_threshold,
                    query_text,
                )
                .await
            }
            "pinecone" => {
                let api_key = std::env::var("PINECONE_API_KEY")
                    .unwrap_or_else(|_| "missing-api-key".to_string());
                let environment = std::env::var("PINECONE_ENVIRONMENT")
                    .unwrap_or_else(|_| "us-west1-gcp".to_string());
                let index_name =
                    std::env::var("PINECONE_INDEX").unwrap_or_else(|_| "default-index".to_string());
                let provider = PineconeProvider::new(api_key, environment, index_name);
                self.search_with_provider(
                    provider,
                    "pinecone",
                    collection,
                    query_vector,
                    top_k,
                    score_threshold,
                    query_text,
                )
                .await
            }
            "weaviate" => {
                let url = std::env::var("WEAVIATE_URL")
                    .unwrap_or_else(|_| "http://localhost:8080".to_string());
                let api_key = std::env::var("WEAVIATE_API_KEY").ok();
                let provider = WeaviateProvider::new(url, api_key);
                self.search_with_provider(
                    provider,
                    "weaviate",
                    collection,
                    query_vector,
                    top_k,
                    score_threshold,
                    query_text,
                )
                .await
            }
            "milvus" => {
                let url = std::env::var("MILVUS_URL")
                    .unwrap_or_else(|_| "http://localhost:19530".to_string());
                let token = std::env::var("MILVUS_TOKEN").ok();
                let provider = MilvusProvider::new(url, token);
                self.search_with_provider(
                    provider,
                    "milvus",
                    collection,
                    query_vector,
                    top_k,
                    score_threshold,
                    query_text,
                )
                .await
            }
            _ => ExecutionResult::Success(serde_json::json!({
                "db_type": db_type,
                "collection": collection,
                "query": query_text,
                "results": [],
                "warning": format!(
                    "Vector DB type '{}' not supported. Supported: qdrant, pgvector, chromadb, pinecone, weaviate, milvus",
                    db_type
                )
            })),
        }
    }

    /// Perform search with a given vector provider
    #[allow(clippy::too_many_arguments)]
    async fn search_with_provider<P: VectorProvider>(
        &self,
        provider: P,
        db_name: &str,
        collection: &str,
        query_vector: &[f32],
        top_k: usize,
        score_threshold: Option<f64>,
        query_text: &str,
    ) -> ExecutionResult {
        match provider
            .search(SearchRequest {
                collection: collection.to_string(),
                query: query_vector.to_vec(),
                top_k,
                score_threshold,
                filter: None,
            })
            .await
        {
            Ok(results) => ExecutionResult::Success(serde_json::json!({
                "db_type": db_name,
                "collection": collection,
                "query": query_text,
                "results": results,
                "count": results.len()
            })),
            Err(e) => ExecutionResult::Failure(format!("{} search error: {}", db_name, e)),
        }
    }

    /// Resolve template variables like {{variable_name}}
    fn resolve_template(&self, template: &str, ctx: &ExecutionContext) -> Result<String> {
        let mut result = template.to_string();

        // Simple template resolution - find {{variable}} patterns
        let re = regex::Regex::new(r"\{\{([^}]+)\}\}")
            .map_err(|e| EngineError::TemplateError(e.to_string()))?;

        for cap in re.captures_iter(template) {
            let var_name = cap.get(1).unwrap().as_str().trim();

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
    fn topological_sort(&self, workflow: &Workflow) -> Result<Vec<NodeId>> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut adj_list: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        // Initialize
        for node in &workflow.nodes {
            in_degree.insert(node.id, 0);
            adj_list.insert(node.id, Vec::new());
        }

        // Build adjacency list and in-degree
        for edge in &workflow.edges {
            adj_list.get_mut(&edge.from).unwrap().push(edge.to);
            *in_degree.get_mut(&edge.to).unwrap() += 1;
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
                    let deg = in_degree.get_mut(&neighbor).unwrap();
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

    /// Compute execution levels for parallel execution
    /// Returns a Vec of levels, where each level contains nodes that can execute in parallel
    fn compute_execution_levels(&self, workflow: &Workflow) -> Result<Vec<Vec<NodeId>>> {
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
            adj_list.get_mut(&edge.from).unwrap().push(edge.to);
            *in_degree.get_mut(&edge.to).unwrap() += 1;
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
                        let deg = in_degree.get_mut(&neighbor).unwrap();
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
        while let Ok(event) = rx.try_recv() {
            event_count += 1;
            // Verify event has correct workflow_id
            assert_eq!(event.workflow_id, workflow.metadata.id);
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
