//! Per-Workflow Cost Tracking
//!
//! This module provides cost tracking and budget enforcement on a per-workflow basis.
//! It allows you to track LLM usage and costs for individual workflows, set per-workflow
//! budgets, and enforce budget limits.
//!
//! # Example
//!
//! ```rust
//! use oxify_connect_llm::{WorkflowProvider, WorkflowTracker, BudgetLimit, OpenAIProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let tracker = WorkflowTracker::new();
//! let provider = OpenAIProvider::new("api-key".to_string(), "gpt-4".to_string());
//!
//! // Wrap with workflow tracking
//! let workflow_provider = WorkflowProvider::new(
//!     provider,
//!     tracker.clone(),
//!     "workflow-123".to_string(),
//!     Some(BudgetLimit::cents(1000)), // $10.00 budget
//! );
//!
//! // Make requests - costs are tracked per workflow
//! // let response = workflow_provider.complete(request).await?;
//!
//! // Check workflow stats
//! let stats = tracker.get_stats("workflow-123");
//! println!("Workflow spent: ${:.2}", stats.total_cost_usd);
//! # Ok(())
//! # }
//! ```

use crate::{
    BudgetLimit, EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmError, LlmProvider,
    LlmRequest, LlmResponse, LlmStream, ModelPricing, Result, StreamingLlmProvider,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Statistics for a workflow
#[derive(Debug, Clone)]
pub struct WorkflowStats {
    /// Workflow ID
    pub workflow_id: String,
    /// Total requests made
    pub total_requests: u64,
    /// Total tokens used (prompt + completion)
    pub total_tokens: u64,
    /// Total prompt tokens
    pub prompt_tokens: u64,
    /// Total completion tokens
    pub completion_tokens: u64,
    /// Total cost in cents
    pub total_cost_cents: u64,
    /// Total cost in USD
    pub total_cost_usd: f64,
    /// Budget limit (if any)
    pub budget_limit: Option<BudgetLimit>,
    /// Budget remaining (if budget set)
    pub budget_remaining_cents: Option<u64>,
    /// Budget remaining percentage (0-100, if budget set)
    pub budget_remaining_percent: Option<f64>,
}

/// Tracks costs and usage per workflow
#[derive(Clone)]
pub struct WorkflowTracker {
    workflows: Arc<Mutex<HashMap<String, WorkflowData>>>,
}

#[derive(Debug, Clone)]
struct WorkflowData {
    requests: u64,
    tokens: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    cost_cents: u64,
    budget: Option<BudgetLimit>,
}

impl WorkflowTracker {
    /// Create a new workflow tracker
    pub fn new() -> Self {
        Self {
            workflows: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Set a budget limit for a workflow
    pub fn set_budget(&self, workflow_id: &str, budget: BudgetLimit) {
        let mut workflows = self.workflows.lock().unwrap();
        workflows
            .entry(workflow_id.to_string())
            .or_insert_with(|| WorkflowData {
                requests: 0,
                tokens: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_cents: 0,
                budget: None,
            })
            .budget = Some(budget);
    }

    /// Record usage for a workflow
    pub fn record_usage(
        &self,
        workflow_id: &str,
        prompt_tokens: u32,
        completion_tokens: u32,
        cost_cents: u64,
    ) {
        let mut workflows = self.workflows.lock().unwrap();
        let data = workflows
            .entry(workflow_id.to_string())
            .or_insert_with(|| WorkflowData {
                requests: 0,
                tokens: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_cents: 0,
                budget: None,
            });

        data.requests += 1;
        data.prompt_tokens += prompt_tokens as u64;
        data.completion_tokens += completion_tokens as u64;
        data.tokens += (prompt_tokens + completion_tokens) as u64;
        data.cost_cents += cost_cents;
    }

    /// Check if a workflow can afford a request (budget check)
    pub fn can_afford(&self, workflow_id: &str, estimated_cost_cents: u64) -> bool {
        let workflows = self.workflows.lock().unwrap();
        if let Some(data) = workflows.get(workflow_id) {
            if let Some(budget) = &data.budget {
                let budget_cents = budget.as_cents();
                return data.cost_cents + estimated_cost_cents <= budget_cents;
            }
        }
        true // No budget = always allowed
    }

    /// Get statistics for a workflow
    pub fn get_stats(&self, workflow_id: &str) -> WorkflowStats {
        let workflows = self.workflows.lock().unwrap();
        if let Some(data) = workflows.get(workflow_id) {
            let budget_remaining_cents = data.budget.as_ref().map(|b| {
                let budget_cents = b.as_cents();
                budget_cents.saturating_sub(data.cost_cents)
            });

            let budget_remaining_percent = data.budget.as_ref().map(|b| {
                let budget_cents = b.as_cents();
                if budget_cents == 0 {
                    0.0
                } else {
                    let remaining = budget_cents.saturating_sub(data.cost_cents);
                    (remaining as f64 / budget_cents as f64) * 100.0
                }
            });

            WorkflowStats {
                workflow_id: workflow_id.to_string(),
                total_requests: data.requests,
                total_tokens: data.tokens,
                prompt_tokens: data.prompt_tokens,
                completion_tokens: data.completion_tokens,
                total_cost_cents: data.cost_cents,
                total_cost_usd: data.cost_cents as f64 / 100.0,
                budget_limit: data.budget.clone(),
                budget_remaining_cents,
                budget_remaining_percent,
            }
        } else {
            WorkflowStats {
                workflow_id: workflow_id.to_string(),
                total_requests: 0,
                total_tokens: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                total_cost_cents: 0,
                total_cost_usd: 0.0,
                budget_limit: None,
                budget_remaining_cents: None,
                budget_remaining_percent: None,
            }
        }
    }

    /// Get statistics for all workflows
    pub fn get_all_stats(&self) -> Vec<WorkflowStats> {
        let workflows = self.workflows.lock().unwrap();
        workflows
            .iter()
            .map(|(workflow_id, data)| {
                let budget_remaining_cents = data.budget.as_ref().map(|b| {
                    let budget_cents = b.as_cents();
                    budget_cents.saturating_sub(data.cost_cents)
                });

                let budget_remaining_percent = data.budget.as_ref().map(|b| {
                    let budget_cents = b.as_cents();
                    if budget_cents == 0 {
                        0.0
                    } else {
                        let remaining = budget_cents.saturating_sub(data.cost_cents);
                        (remaining as f64 / budget_cents as f64) * 100.0
                    }
                });

                WorkflowStats {
                    workflow_id: workflow_id.to_string(),
                    total_requests: data.requests,
                    total_tokens: data.tokens,
                    prompt_tokens: data.prompt_tokens,
                    completion_tokens: data.completion_tokens,
                    total_cost_cents: data.cost_cents,
                    total_cost_usd: data.cost_cents as f64 / 100.0,
                    budget_limit: data.budget.clone(),
                    budget_remaining_cents,
                    budget_remaining_percent,
                }
            })
            .collect()
    }

    /// Reset statistics for a workflow
    pub fn reset(&self, workflow_id: &str) {
        let mut workflows = self.workflows.lock().unwrap();
        if let Some(data) = workflows.get_mut(workflow_id) {
            let budget = data.budget.clone();
            *data = WorkflowData {
                requests: 0,
                tokens: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_cents: 0,
                budget,
            };
        }
    }

    /// Reset all workflow statistics
    pub fn reset_all(&self) {
        let mut workflows = self.workflows.lock().unwrap();
        for data in workflows.values_mut() {
            let budget = data.budget.clone();
            *data = WorkflowData {
                requests: 0,
                tokens: 0,
                prompt_tokens: 0,
                completion_tokens: 0,
                cost_cents: 0,
                budget,
            };
        }
    }

    /// Remove a workflow from tracking
    pub fn remove(&self, workflow_id: &str) {
        let mut workflows = self.workflows.lock().unwrap();
        workflows.remove(workflow_id);
    }
}

impl Default for WorkflowTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// LLM provider wrapper that tracks costs per workflow
pub struct WorkflowProvider<P> {
    provider: P,
    tracker: WorkflowTracker,
    workflow_id: String,
    pricing: ModelPricing,
}

impl<P> WorkflowProvider<P> {
    /// Create a new workflow provider
    pub fn new(
        provider: P,
        tracker: WorkflowTracker,
        workflow_id: String,
        budget: Option<BudgetLimit>,
    ) -> Self {
        // Set budget in tracker if provided
        if let Some(ref budget_limit) = budget {
            tracker.set_budget(&workflow_id, budget_limit.clone());
        }

        Self {
            provider,
            tracker,
            workflow_id,
            pricing: ModelPricing::gpt4(),
        }
    }

    /// Set the pricing model for cost calculation
    pub fn with_pricing(mut self, pricing: ModelPricing) -> Self {
        self.pricing = pricing;
        self
    }

    /// Get the workflow tracker
    pub fn tracker(&self) -> &WorkflowTracker {
        &self.tracker
    }

    /// Get the workflow ID
    pub fn workflow_id(&self) -> &str {
        &self.workflow_id
    }

    /// Get statistics for this workflow
    pub fn stats(&self) -> WorkflowStats {
        self.tracker.get_stats(&self.workflow_id)
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for WorkflowProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Estimate cost before making request
        let estimated_tokens =
            request.prompt.len() / 4 + request.max_tokens.unwrap_or(1000) as usize;
        let estimated_cost_cents = self
            .pricing
            .calculate_cost((estimated_tokens / 2) as u32, (estimated_tokens / 2) as u32)
            as u64;

        // Check budget
        if !self
            .tracker
            .can_afford(&self.workflow_id, estimated_cost_cents)
        {
            return Err(LlmError::Other(format!(
                "Workflow '{}' has exceeded its budget limit",
                self.workflow_id
            )));
        }

        // Make the request
        let response = self.provider.complete(request).await?;

        // Record actual usage
        if let Some(usage) = &response.usage {
            let actual_cost_cents = self
                .pricing
                .calculate_cost(usage.prompt_tokens, usage.completion_tokens)
                as u64;
            self.tracker.record_usage(
                &self.workflow_id,
                usage.prompt_tokens,
                usage.completion_tokens,
                actual_cost_cents,
            );
        }

        Ok(response)
    }
}

#[async_trait]
impl<P: StreamingLlmProvider> StreamingLlmProvider for WorkflowProvider<P> {
    async fn complete_stream(&self, request: LlmRequest) -> Result<LlmStream> {
        // Estimate cost before making request
        let estimated_tokens =
            request.prompt.len() / 4 + request.max_tokens.unwrap_or(1000) as usize;
        let estimated_cost_cents = self
            .pricing
            .calculate_cost((estimated_tokens / 2) as u32, (estimated_tokens / 2) as u32)
            as u64;

        // Check budget
        if !self
            .tracker
            .can_afford(&self.workflow_id, estimated_cost_cents)
        {
            return Err(LlmError::Other(format!(
                "Workflow '{}' has exceeded its budget limit",
                self.workflow_id
            )));
        }

        // Note: For streaming, we can't track exact usage until the stream completes
        // This is a limitation of the streaming API
        self.provider.complete_stream(request).await
    }
}

/// Embedding provider wrapper that tracks costs per workflow
pub struct WorkflowEmbeddingProvider<P> {
    provider: P,
    tracker: WorkflowTracker,
    workflow_id: String,
    pricing: ModelPricing,
}

impl<P> WorkflowEmbeddingProvider<P> {
    /// Create a new workflow embedding provider
    pub fn new(provider: P, tracker: WorkflowTracker, workflow_id: String) -> Self {
        Self {
            provider,
            tracker,
            workflow_id,
            pricing: ModelPricing::ada_embedding(),
        }
    }

    /// Set the pricing model for cost calculation
    pub fn with_pricing(mut self, pricing: ModelPricing) -> Self {
        self.pricing = pricing;
        self
    }

    /// Get the workflow tracker
    pub fn tracker(&self) -> &WorkflowTracker {
        &self.tracker
    }

    /// Get statistics for this workflow
    pub fn stats(&self) -> WorkflowStats {
        self.tracker.get_stats(&self.workflow_id)
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for WorkflowEmbeddingProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let response = self.provider.embed(request).await?;

        // Record usage
        if let Some(usage) = &response.usage {
            let cost_cents = self.pricing.calculate_cost(usage.prompt_tokens, 0) as u64;
            self.tracker
                .record_usage(&self.workflow_id, usage.prompt_tokens, 0, cost_cents);
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_tracker_new() {
        let tracker = WorkflowTracker::new();
        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.total_tokens, 0);
        assert_eq!(stats.total_cost_cents, 0);
    }

    #[test]
    fn test_workflow_tracker_record_usage() {
        let tracker = WorkflowTracker::new();

        // Record some usage
        tracker.record_usage("workflow-1", 100, 50, 10);
        tracker.record_usage("workflow-1", 200, 100, 20);

        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.prompt_tokens, 300);
        assert_eq!(stats.completion_tokens, 150);
        assert_eq!(stats.total_tokens, 450);
        assert_eq!(stats.total_cost_cents, 30);
        assert_eq!(stats.total_cost_usd, 0.30);
    }

    #[test]
    fn test_workflow_tracker_multiple_workflows() {
        let tracker = WorkflowTracker::new();

        tracker.record_usage("workflow-1", 100, 50, 10);
        tracker.record_usage("workflow-2", 200, 100, 20);
        tracker.record_usage("workflow-1", 50, 25, 5);

        let stats1 = tracker.get_stats("workflow-1");
        assert_eq!(stats1.total_requests, 2);
        assert_eq!(stats1.total_cost_cents, 15);

        let stats2 = tracker.get_stats("workflow-2");
        assert_eq!(stats2.total_requests, 1);
        assert_eq!(stats2.total_cost_cents, 20);

        let all_stats = tracker.get_all_stats();
        assert_eq!(all_stats.len(), 2);
    }

    #[test]
    fn test_workflow_budget() {
        let tracker = WorkflowTracker::new();

        // Set a budget of $1.00 (100 cents)
        tracker.set_budget("workflow-1", BudgetLimit::cents(100));

        // Record usage
        tracker.record_usage("workflow-1", 100, 50, 30);

        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.budget_limit, Some(BudgetLimit::cents(100)));
        assert_eq!(stats.budget_remaining_cents, Some(70));
        assert_eq!(stats.budget_remaining_percent, Some(70.0));
    }

    #[test]
    fn test_workflow_can_afford() {
        let tracker = WorkflowTracker::new();

        // Set a budget of $1.00 (100 cents)
        tracker.set_budget("workflow-1", BudgetLimit::cents(100));

        // Can afford 50 cents
        assert!(tracker.can_afford("workflow-1", 50));

        // Record 80 cents usage
        tracker.record_usage("workflow-1", 100, 50, 80);

        // Can afford 20 cents
        assert!(tracker.can_afford("workflow-1", 20));

        // Cannot afford 21 cents (would exceed budget)
        assert!(!tracker.can_afford("workflow-1", 21));

        // Workflow without budget can always afford
        assert!(tracker.can_afford("workflow-2", 1000000));
    }

    #[test]
    fn test_workflow_reset() {
        let tracker = WorkflowTracker::new();

        tracker.record_usage("workflow-1", 100, 50, 10);
        tracker.set_budget("workflow-1", BudgetLimit::cents(100));

        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.total_requests, 1);

        // Reset workflow
        tracker.reset("workflow-1");

        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.total_requests, 0);
        assert_eq!(stats.total_cost_cents, 0);
        // Budget should be preserved
        assert_eq!(stats.budget_limit, Some(BudgetLimit::cents(100)));
    }

    #[test]
    fn test_workflow_reset_all() {
        let tracker = WorkflowTracker::new();

        tracker.record_usage("workflow-1", 100, 50, 10);
        tracker.record_usage("workflow-2", 200, 100, 20);

        tracker.reset_all();

        let stats1 = tracker.get_stats("workflow-1");
        let stats2 = tracker.get_stats("workflow-2");
        assert_eq!(stats1.total_requests, 0);
        assert_eq!(stats2.total_requests, 0);
    }

    #[test]
    fn test_workflow_remove() {
        let tracker = WorkflowTracker::new();

        tracker.record_usage("workflow-1", 100, 50, 10);
        tracker.record_usage("workflow-2", 200, 100, 20);

        tracker.remove("workflow-1");

        let all_stats = tracker.get_all_stats();
        assert_eq!(all_stats.len(), 1);
        assert_eq!(all_stats[0].workflow_id, "workflow-2");
    }

    #[test]
    fn test_workflow_budget_exceeded() {
        let tracker = WorkflowTracker::new();

        // Set a budget of $0.50 (50 cents)
        tracker.set_budget("workflow-1", BudgetLimit::cents(50));

        // Record usage that exceeds budget
        tracker.record_usage("workflow-1", 100, 50, 60);

        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.budget_remaining_cents, Some(0)); // Saturating sub
        assert_eq!(stats.budget_remaining_percent, Some(0.0));
    }

    #[test]
    fn test_workflow_stats_no_budget() {
        let tracker = WorkflowTracker::new();

        tracker.record_usage("workflow-1", 100, 50, 10);

        let stats = tracker.get_stats("workflow-1");
        assert_eq!(stats.budget_limit, None);
        assert_eq!(stats.budget_remaining_cents, None);
        assert_eq!(stats.budget_remaining_percent, None);
    }
}
