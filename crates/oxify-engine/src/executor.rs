//! Top-level workflow execution methods for the Engine.
//!
//! Contains `execute`, `execute_sequential`, `execute_with_config`,
//! `execute_from_checkpoint` and all the level-execution helpers.

use super::*;

impl Engine {
    /// Core execution body — receives a pre-built [`ExecutionContext`] so the
    /// caller controls the `execution_id` (enables SSE correlation).
    ///
    /// Every public entry-point (`execute_with_config`, `execute_with_context`)
    /// is a thin wrapper around this method.
    async fn run(
        &self,
        workflow: &Workflow,
        mut ctx: ExecutionContext,
        config: ExecutionConfig,
    ) -> Result<ExecutionContext> {
        // Validate workflow first
        workflow.validate().map_err(EngineError::ValidationError)?;

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

        tracing::info!(duration_ms = duration_ms as u64, "workflow completed");

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

    /// Execute workflow from a checkpoint
    pub(super) async fn execute_from_checkpoint(
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
    #[tracing::instrument(
        name = "oxify.workflow.execute",
        skip(self, workflow, config),
        fields(
            workflow.id = %workflow.metadata.id,
            workflow.name = %workflow.metadata.name,
            workflow.nodes = workflow.nodes.len(),
        )
    )]
    pub async fn execute_with_config(
        &self,
        workflow: &Workflow,
        config: ExecutionConfig,
    ) -> Result<ExecutionContext> {
        let ctx = ExecutionContext::new(workflow.metadata.id);
        self.run(workflow, ctx, config).await
    }

    /// Execute a workflow reusing a caller-supplied [`ExecutionContext`].
    ///
    /// Preserves the caller's `execution_id` so every emitted [`WorkflowEvent`]
    /// carries that id, enabling end-to-end SSE correlation.
    pub async fn execute_with_context(
        &self,
        workflow: &Workflow,
        ctx: ExecutionContext,
        config: ExecutionConfig,
    ) -> Result<ExecutionContext> {
        self.run(workflow, ctx, config).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::{execution_events, EventBus};
    use oxify_model::{Edge, Node, NodeKind, Workflow};
    use std::sync::Arc;

    fn build_start_end_workflow() -> Workflow {
        let mut workflow = Workflow::new("Test Workflow".to_string());
        let start = Node::new("Start".to_string(), NodeKind::Start);
        let end = Node::new("End".to_string(), NodeKind::End);
        let start_id = start.id;
        let end_id = end.id;
        workflow.add_node(start);
        workflow.add_node(end);
        workflow.add_edge(Edge::new(start_id, end_id));
        workflow
    }

    #[tokio::test]
    async fn test_execute_with_context_preserves_id_and_emits_events() {
        let workflow = build_start_end_workflow();

        let event_bus = Arc::new(EventBus::new(100));
        let mut rx = event_bus.subscribe();
        let engine = EngineBuilder::new().with_event_bus(event_bus).build();

        let ctx = ExecutionContext::new(workflow.metadata.id);
        let chosen_id = ctx.execution_id;

        let result = engine
            .execute_with_context(&workflow, ctx, ExecutionConfig::new().with_events())
            .await
            .unwrap();

        // Execution id must be preserved
        assert_eq!(result.execution_id, chosen_id);
        assert!(matches!(result.state, ExecutionState::Completed));

        // Drain broadcast buffer; events pushed synchronously before await point
        let mut saw_started = false;
        let mut saw_completed = false;
        while let Ok(ev) = rx.try_recv() {
            assert_eq!(
                ev.execution_id,
                Some(chosen_id),
                "every event must carry the caller's execution_id"
            );
            match ev.event_type.as_str() {
                s if s == execution_events::WORKFLOW_STARTED => saw_started = true,
                s if s == execution_events::WORKFLOW_COMPLETED => saw_completed = true,
                _ => {}
            }
        }
        assert!(saw_started, "expected workflow.started event");
        assert!(saw_completed, "expected workflow.completed event");
    }

    #[tokio::test]
    async fn test_execute_with_context_new_ctx_each_call() {
        // Calling execute_with_context twice with two different contexts
        // must yield two different execution ids.
        let workflow = build_start_end_workflow();
        let engine = Engine::new();

        let ctx_a = ExecutionContext::new(workflow.metadata.id);
        let ctx_b = ExecutionContext::new(workflow.metadata.id);
        let id_a = ctx_a.execution_id;
        let id_b = ctx_b.execution_id;
        assert_ne!(id_a, id_b);

        let res_a = engine
            .execute_with_context(&workflow, ctx_a, ExecutionConfig::default())
            .await
            .unwrap();
        let res_b = engine
            .execute_with_context(&workflow, ctx_b, ExecutionConfig::default())
            .await
            .unwrap();

        assert_eq!(res_a.execution_id, id_a);
        assert_eq!(res_b.execution_id, id_b);
    }
}
