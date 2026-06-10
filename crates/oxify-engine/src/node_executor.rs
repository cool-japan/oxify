//! Per-node execution dispatch for the Engine.
//!
//! Contains `execute_node_with_retry`, `execute_node` (the NodeKind match arm),
//! and all `execute_*_node` / `execute_vector_search` helpers.

use super::*;

impl Engine {
    /// Execute a node with retry logic
    #[tracing::instrument(
        name = "oxify.node.execute",
        skip(self, node, ctx, workflow),
        fields(
            node.id = ?node.id,
            node.name = %node.name,
        )
    )]
    pub(super) async fn execute_node_with_retry(
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
    #[tracing::instrument(
        name = "oxify.node.step",
        skip(self, node, ctx, _workflow),
        fields(
            node.id = ?node.id,
        )
    )]
    pub(super) async fn execute_node(
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

            NodeKind::Custom(cfg) => {
                // Dispatch to the registered plugin for this plugin_id
                match self.plugin_registry.get(&cfg.plugin_id).await {
                    Some(plugin) => {
                        // Validate node configuration before executing
                        plugin.validate(node).map_err(|e| {
                            EngineError::ExecutionError(format!(
                                "Plugin '{}' validation failed for node '{}': {}",
                                cfg.plugin_id, node.name, e
                            ))
                        })?;

                        // Execute via plugin and convert result
                        match plugin.execute(node, ctx).await {
                            Ok(exec_result) => exec_result,
                            Err(e) => ExecutionResult::Failure(format!(
                                "Plugin '{}' execution error: {}",
                                cfg.plugin_id, e
                            )),
                        }
                    }
                    None => {
                        return Err(EngineError::ExecutionError(format!(
                            "No plugin registered for plugin_id: '{}'",
                            cfg.plugin_id
                        )));
                    }
                }
            }
        };

        Ok(node_result.complete(result))
    }

    /// Check if all dependencies of a node are satisfied
    #[allow(dead_code)]
    pub(super) fn dependencies_satisfied(
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
    pub(super) async fn execute_vector_search(
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
    pub(super) async fn search_with_provider<P: VectorProvider>(
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{NodePlugin, PluginRegistry};
    use oxify_model::{CustomConfig, ExecutionResult, Node, NodeKind};
    use std::sync::Arc;

    /// A trivial plugin that always returns success with a known payload
    struct EchoPlugin;

    #[async_trait::async_trait]
    impl NodePlugin for EchoPlugin {
        fn name(&self) -> &str {
            "echo"
        }

        fn version(&self) -> &str {
            "1.0.0"
        }

        fn supported_node_types(&self) -> Vec<String> {
            vec!["echo".to_string()]
        }

        async fn execute(
            &self,
            _node: &Node,
            _context: &ExecutionContext,
        ) -> std::result::Result<ExecutionResult, String> {
            Ok(ExecutionResult::Success(serde_json::json!({
                "plugin": "echo",
                "status": "executed"
            })))
        }
    }

    #[tokio::test]
    async fn test_custom_node_plugin_dispatch() {
        // Build a registry with the echo plugin
        let registry = Arc::new(PluginRegistry::new());
        registry
            .register(Arc::new(EchoPlugin))
            .await
            .expect("register echo plugin");

        let engine = Engine::builder().with_plugin_registry(registry).build();

        let node = Node::new(
            "echo_node".to_string(),
            NodeKind::Custom(CustomConfig {
                plugin_id: "echo".to_string(),
                plugin_version: None,
                config: serde_json::Value::Null,
            }),
        );

        let ctx = ExecutionContext::new(uuid::Uuid::new_v4());
        let workflow = oxify_model::Workflow::new("test".to_string());

        let result = engine
            .execute_node(&node, &ctx, &workflow)
            .await
            .expect("execute custom node");

        assert!(matches!(result.result, ExecutionResult::Success(_)));
    }

    #[tokio::test]
    async fn test_custom_node_unknown_plugin_returns_error() {
        let engine = Engine::new(); // empty registry

        let node = Node::new(
            "unknown_node".to_string(),
            NodeKind::Custom(CustomConfig {
                plugin_id: "nonexistent_plugin".to_string(),
                plugin_version: None,
                config: serde_json::Value::Null,
            }),
        );

        let ctx = ExecutionContext::new(uuid::Uuid::new_v4());
        let workflow = oxify_model::Workflow::new("test".to_string());

        let err = engine
            .execute_node(&node, &ctx, &workflow)
            .await
            .expect_err("should return error for unknown plugin");

        assert!(
            err.to_string().contains("nonexistent_plugin"),
            "error message should mention the plugin_id"
        );
    }

    #[test]
    fn test_custom_config_serde_round_trip() {
        let original = CustomConfig {
            plugin_id: "my.plugin".to_string(),
            plugin_version: Some("2.1.0".to_string()),
            config: serde_json::json!({"key": "value", "count": 42}),
        };

        let json = serde_json::to_string(&original).expect("serialize CustomConfig");
        let restored: CustomConfig = serde_json::from_str(&json).expect("deserialize CustomConfig");

        assert_eq!(restored.plugin_id, original.plugin_id);
        assert_eq!(restored.plugin_version, original.plugin_version);
        assert_eq!(restored.config, original.config);
    }
}
