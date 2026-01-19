//! Usage monitoring and cost tracking for LLM providers

use crate::{
    EmbeddingProvider, EmbeddingRequest, EmbeddingResponse, LlmError, LlmProvider, LlmRequest,
    LlmResponse, Result, Usage,
};
use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Cost per 1K tokens for different providers/models (in USD cents)
/// These are approximate values and should be updated as pricing changes
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// Cost per 1K input/prompt tokens (in cents)
    pub input_per_1k: f64,
    /// Cost per 1K output/completion tokens (in cents)
    pub output_per_1k: f64,
}

impl ModelPricing {
    /// Create custom pricing
    pub const fn new(input_per_1k: f64, output_per_1k: f64) -> Self {
        Self {
            input_per_1k,
            output_per_1k,
        }
    }

    /// GPT-4 (original) pricing
    pub const GPT4: Self = Self::new(3.0, 6.0); // $0.03/$0.06 per 1K

    /// GPT-4 Turbo pricing (as of 2026)
    pub const GPT4_TURBO: Self = Self::new(1.0, 3.0); // $0.01/$0.03 per 1K

    /// GPT-4o pricing (as of 2026)
    pub const GPT4O: Self = Self::new(0.5, 1.5); // $0.005/$0.015 per 1K

    /// GPT-4o-mini pricing
    pub const GPT4O_MINI: Self = Self::new(0.015, 0.06); // $0.00015/$0.0006 per 1K

    /// o1-preview pricing (reasoning model)
    pub const O1_PREVIEW: Self = Self::new(1.5, 6.0); // $0.015/$0.06 per 1K

    /// o1-mini pricing (reasoning model)
    pub const O1_MINI: Self = Self::new(0.3, 1.2); // $0.003/$0.012 per 1K

    /// GPT-3.5 Turbo pricing
    pub const GPT35_TURBO: Self = Self::new(0.05, 0.15); // $0.0005/$0.0015 per 1K

    /// Claude 3 Opus pricing
    pub const CLAUDE3_OPUS: Self = Self::new(1.5, 7.5); // $0.015/$0.075 per 1K

    /// Claude 3.5 Sonnet pricing (newer model)
    pub const CLAUDE35_SONNET: Self = Self::new(0.3, 1.5); // $0.003/$0.015 per 1K

    /// Claude 3 Sonnet pricing (original)
    pub const CLAUDE3_SONNET: Self = Self::new(0.3, 1.5); // $0.003/$0.015 per 1K

    /// Claude 3.5 Haiku pricing (newer model)
    pub const CLAUDE35_HAIKU: Self = Self::new(0.08, 0.4); // $0.0008/$0.004 per 1K

    /// Claude 3 Haiku pricing (original)
    pub const CLAUDE3_HAIKU: Self = Self::new(0.025, 0.125); // $0.00025/$0.00125 per 1K

    /// Gemini Pro pricing
    pub const GEMINI_PRO: Self = Self::new(0.0125, 0.0375); // Free tier, then $0.000125/$0.000375

    /// Gemini Flash pricing (faster, cheaper)
    pub const GEMINI_FLASH: Self = Self::new(0.00375, 0.01125); // $0.0000375/$0.0001125 per 1K

    /// Mistral Large pricing
    pub const MISTRAL_LARGE: Self = Self::new(0.2, 0.6); // $0.002/$0.006 per 1K

    /// Mistral Small pricing
    pub const MISTRAL_SMALL: Self = Self::new(0.1, 0.3); // $0.001/$0.003 per 1K

    /// Cohere Command-R pricing
    pub const COHERE_COMMAND_R: Self = Self::new(0.05, 0.15); // $0.0005/$0.0015 per 1K

    /// Cohere Command-R+ pricing (larger model)
    pub const COHERE_COMMAND_R_PLUS: Self = Self::new(0.3, 1.5); // $0.003/$0.015 per 1K

    /// Free/local models (Ollama, etc.)
    pub const FREE: Self = Self::new(0.0, 0.0);

    /// OpenAI Ada embedding pricing (text-embedding-ada-002)
    pub const ADA_EMBEDDING: Self = Self::new(0.01, 0.0); // $0.0001 per 1K tokens

    /// OpenAI text-embedding-3-small pricing
    pub const TEXT_EMBEDDING_3_SMALL: Self = Self::new(0.002, 0.0); // $0.00002 per 1K tokens

    /// OpenAI text-embedding-3-large pricing
    pub const TEXT_EMBEDDING_3_LARGE: Self = Self::new(0.013, 0.0); // $0.00013 per 1K tokens

    /// Calculate cost in cents for given token counts
    pub fn calculate_cost(&self, prompt_tokens: u32, completion_tokens: u32) -> f64 {
        let input_cost = (prompt_tokens as f64 / 1000.0) * self.input_per_1k;
        let output_cost = (completion_tokens as f64 / 1000.0) * self.output_per_1k;
        input_cost + output_cost
    }

    /// Helper function to get GPT-4 pricing
    pub const fn gpt4() -> Self {
        Self::GPT4
    }

    /// Helper function to get Ada embedding pricing
    pub const fn ada_embedding() -> Self {
        Self::ADA_EMBEDDING
    }
}

/// Accumulated usage statistics
#[derive(Debug, Clone)]
pub struct UsageStats {
    /// Total prompt/input tokens
    pub total_prompt_tokens: u64,
    /// Total completion/output tokens
    pub total_completion_tokens: u64,
    /// Total tokens (prompt + completion)
    pub total_tokens: u64,
    /// Number of requests
    pub request_count: u64,
    /// Estimated cost in cents (if pricing is set)
    pub estimated_cost_cents: f64,
}

impl UsageStats {
    /// Get estimated cost in dollars
    pub fn estimated_cost_usd(&self) -> f64 {
        self.estimated_cost_cents / 100.0
    }

    /// Get average tokens per request
    pub fn avg_tokens_per_request(&self) -> f64 {
        if self.request_count == 0 {
            0.0
        } else {
            self.total_tokens as f64 / self.request_count as f64
        }
    }
}

/// Thread-safe usage tracker
#[derive(Debug)]
pub struct UsageTracker {
    prompt_tokens: AtomicU64,
    completion_tokens: AtomicU64,
    request_count: AtomicU64,
    pricing: Option<ModelPricing>,
}

impl Clone for UsageTracker {
    fn clone(&self) -> Self {
        Self {
            prompt_tokens: AtomicU64::new(self.prompt_tokens.load(Ordering::Relaxed)),
            completion_tokens: AtomicU64::new(self.completion_tokens.load(Ordering::Relaxed)),
            request_count: AtomicU64::new(self.request_count.load(Ordering::Relaxed)),
            pricing: self.pricing,
        }
    }
}

impl Default for UsageTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageTracker {
    /// Create a new usage tracker without pricing
    pub fn new() -> Self {
        Self {
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
            request_count: AtomicU64::new(0),
            pricing: None,
        }
    }

    /// Create a new usage tracker with pricing
    pub fn with_pricing(pricing: ModelPricing) -> Self {
        Self {
            prompt_tokens: AtomicU64::new(0),
            completion_tokens: AtomicU64::new(0),
            request_count: AtomicU64::new(0),
            pricing: Some(pricing),
        }
    }

    /// Record usage from an LLM response
    pub fn record(&self, usage: &Usage) {
        self.prompt_tokens
            .fetch_add(usage.prompt_tokens as u64, Ordering::Relaxed);
        self.completion_tokens
            .fetch_add(usage.completion_tokens as u64, Ordering::Relaxed);
        self.request_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current usage statistics
    pub fn stats(&self) -> UsageStats {
        let prompt = self.prompt_tokens.load(Ordering::Relaxed);
        let completion = self.completion_tokens.load(Ordering::Relaxed);
        let count = self.request_count.load(Ordering::Relaxed);

        let cost = self
            .pricing
            .map(|p| p.calculate_cost(prompt as u32, completion as u32))
            .unwrap_or(0.0);

        UsageStats {
            total_prompt_tokens: prompt,
            total_completion_tokens: completion,
            total_tokens: prompt + completion,
            request_count: count,
            estimated_cost_cents: cost,
        }
    }

    /// Reset all counters
    pub fn reset(&self) {
        self.prompt_tokens.store(0, Ordering::Relaxed);
        self.completion_tokens.store(0, Ordering::Relaxed);
        self.request_count.store(0, Ordering::Relaxed);
    }
}

/// A wrapper that tracks usage for any LLM provider
pub struct TrackedProvider<P> {
    inner: P,
    tracker: Arc<UsageTracker>,
}

impl<P> TrackedProvider<P> {
    /// Create a new TrackedProvider without pricing
    pub fn new(provider: P) -> Self {
        Self {
            inner: provider,
            tracker: Arc::new(UsageTracker::new()),
        }
    }

    /// Create a new TrackedProvider with pricing
    pub fn with_pricing(provider: P, pricing: ModelPricing) -> Self {
        Self {
            inner: provider,
            tracker: Arc::new(UsageTracker::with_pricing(pricing)),
        }
    }

    /// Create a new TrackedProvider with a shared tracker
    pub fn with_tracker(provider: P, tracker: Arc<UsageTracker>) -> Self {
        Self {
            inner: provider,
            tracker,
        }
    }

    /// Get a reference to the inner provider
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Get a mutable reference to the inner provider
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    /// Get a reference to the usage tracker
    pub fn tracker(&self) -> &Arc<UsageTracker> {
        &self.tracker
    }

    /// Get current usage statistics
    pub fn stats(&self) -> UsageStats {
        self.tracker.stats()
    }

    /// Reset usage counters
    pub fn reset(&self) {
        self.tracker.reset();
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for TrackedProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        let response = self.inner.complete(request).await?;

        // Track usage if available
        if let Some(usage) = &response.usage {
            self.tracker.record(usage);
        }

        Ok(response)
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for TrackedProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        let response = self.inner.embed(request).await?;

        // Track usage if available
        if let Some(usage) = &response.usage {
            self.tracker.record(&Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: 0,
                total_tokens: usage.total_tokens,
            });
        }

        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_pricing_calculation() {
        let pricing = ModelPricing::GPT4_TURBO;

        // 1000 input + 500 output tokens
        let cost = pricing.calculate_cost(1000, 500);

        // 1.0 cents for input + 1.5 cents for output = 2.5 cents
        assert!((cost - 2.5).abs() < 0.001);
    }

    #[test]
    fn test_usage_tracker() {
        let tracker = UsageTracker::with_pricing(ModelPricing::GPT35_TURBO);

        tracker.record(&Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        });

        let stats = tracker.stats();
        assert_eq!(stats.total_prompt_tokens, 100);
        assert_eq!(stats.total_completion_tokens, 50);
        assert_eq!(stats.total_tokens, 150);
        assert_eq!(stats.request_count, 1);

        // Record another request
        tracker.record(&Usage {
            prompt_tokens: 200,
            completion_tokens: 100,
            total_tokens: 300,
        });

        let stats = tracker.stats();
        assert_eq!(stats.total_prompt_tokens, 300);
        assert_eq!(stats.total_completion_tokens, 150);
        assert_eq!(stats.total_tokens, 450);
        assert_eq!(stats.request_count, 2);
        assert_eq!(stats.avg_tokens_per_request(), 225.0);
    }

    #[test]
    fn test_usage_tracker_reset() {
        let tracker = UsageTracker::new();

        tracker.record(&Usage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        });

        assert_eq!(tracker.stats().total_tokens, 150);

        tracker.reset();

        assert_eq!(tracker.stats().total_tokens, 0);
        assert_eq!(tracker.stats().request_count, 0);
    }

    #[test]
    fn test_free_pricing() {
        let pricing = ModelPricing::FREE;
        let cost = pricing.calculate_cost(10000, 5000);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn test_usage_stats_usd() {
        let tracker = UsageTracker::with_pricing(ModelPricing::new(100.0, 100.0)); // 1 dollar per 1K

        tracker.record(&Usage {
            prompt_tokens: 1000,
            completion_tokens: 1000,
            total_tokens: 2000,
        });

        let stats = tracker.stats();
        // 100 cents input + 100 cents output = 200 cents = $2.00
        assert_eq!(stats.estimated_cost_cents, 200.0);
        assert_eq!(stats.estimated_cost_usd(), 2.0);
    }

    #[tokio::test]
    async fn test_budget_provider_under_limit() {
        struct MockProvider;
        #[async_trait]
        impl LlmProvider for MockProvider {
            async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
                Ok(LlmResponse {
                    content: format!("Response to: {}", request.prompt),
                    model: "mock".to_string(),
                    usage: Some(Usage {
                        prompt_tokens: 100,
                        completion_tokens: 50,
                        total_tokens: 150,
                    }),
                    tool_calls: Vec::new(),
                })
            }
        }

        let budget = BudgetLimit::new(100.0); // $1.00 budget
        let provider = BudgetProvider::new(MockProvider, budget, ModelPricing::new(10.0, 10.0));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        let result = provider.complete(request).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_budget_provider_exceeds_limit() {
        struct MockProvider;
        #[async_trait]
        impl LlmProvider for MockProvider {
            async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
                Ok(LlmResponse {
                    content: format!("Response to: {}", request.prompt),
                    model: "mock".to_string(),
                    usage: Some(Usage {
                        prompt_tokens: 10000,
                        completion_tokens: 5000,
                        total_tokens: 15000,
                    }),
                    tool_calls: Vec::new(),
                })
            }
        }

        let budget = BudgetLimit::new(0.5); // $0.005 budget (very small)
        let provider = BudgetProvider::new(MockProvider, budget, ModelPricing::new(100.0, 100.0));

        let request = LlmRequest {
            prompt: "test".to_string(),
            system_prompt: None,
            temperature: None,
            max_tokens: None,
            tools: Vec::new(),
            images: Vec::new(),
        };

        // First request should succeed
        let result = provider.complete(request.clone()).await;
        assert!(result.is_ok());

        // Second request should fail due to budget exceeded
        let result = provider.complete(request).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ApiError(_)));
    }

    #[test]
    fn test_budget_limit_remaining() {
        let budget = BudgetLimit::new(100.0);
        assert_eq!(budget.remaining_cents(), 100.0);

        budget.consume(50.0);
        assert_eq!(budget.remaining_cents(), 50.0);

        budget.reset();
        assert_eq!(budget.remaining_cents(), 100.0);
    }
}

// ===== Budget Limits =====

/// Budget limit configuration
#[derive(Debug, Clone)]
pub struct BudgetLimit {
    /// Maximum budget in cents
    max_budget_cents: f64,
    /// Consumed budget tracker (atomic for thread safety)
    consumed_cents: Arc<AtomicU64>,
}

impl PartialEq for BudgetLimit {
    fn eq(&self, other: &Self) -> bool {
        self.max_budget_cents == other.max_budget_cents
    }
}

impl BudgetLimit {
    /// Create a new budget limit in cents
    pub fn new(max_budget_cents: f64) -> Self {
        Self {
            max_budget_cents,
            consumed_cents: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Create a new budget limit in cents
    pub fn cents(max_budget_cents: u64) -> Self {
        Self::new(max_budget_cents as f64)
    }

    /// Create a new budget limit in dollars
    pub fn from_usd(max_budget_usd: f64) -> Self {
        Self::new(max_budget_usd * 100.0)
    }

    /// Create a new budget limit in dollars (alias for from_usd)
    pub fn dollars(max_budget_usd: u64) -> Self {
        Self::from_usd(max_budget_usd as f64)
    }

    /// Get the maximum budget in cents
    pub fn as_cents(&self) -> u64 {
        self.max_budget_cents as u64
    }

    /// Get the maximum budget in dollars
    pub fn as_usd(&self) -> f64 {
        self.max_budget_cents / 100.0
    }

    /// Check if budget has been exceeded
    pub fn is_exceeded(&self) -> bool {
        let consumed = self.consumed_cents();
        consumed >= self.max_budget_cents
    }

    /// Get remaining budget in cents
    pub fn remaining_cents(&self) -> f64 {
        let consumed = self.consumed_cents();
        (self.max_budget_cents - consumed).max(0.0)
    }

    /// Get remaining budget in dollars
    pub fn remaining_usd(&self) -> f64 {
        self.remaining_cents() / 100.0
    }

    /// Get consumed budget in cents
    pub fn consumed_cents(&self) -> f64 {
        // Stored as u64 with 2 decimal places (cents as integer)
        let bits = self.consumed_cents.load(Ordering::Relaxed);
        f64::from_bits(bits)
    }

    /// Consume budget (internal use)
    fn consume(&self, cents: f64) {
        // Use atomic operations to update consumed budget
        let current = self.consumed_cents();
        let new_value = current + cents;
        self.consumed_cents
            .store(new_value.to_bits(), Ordering::Relaxed);
    }

    /// Reset consumed budget
    pub fn reset(&self) {
        self.consumed_cents
            .store(0_f64.to_bits(), Ordering::Relaxed);
    }
}

/// A provider that enforces budget limits
pub struct BudgetProvider<P> {
    inner: P,
    budget: BudgetLimit,
    pricing: ModelPricing,
}

impl<P> BudgetProvider<P> {
    /// Create a new budget-limited provider
    pub fn new(provider: P, budget: BudgetLimit, pricing: ModelPricing) -> Self {
        Self {
            inner: provider,
            budget,
            pricing,
        }
    }

    /// Get a reference to the inner provider
    pub fn inner(&self) -> &P {
        &self.inner
    }

    /// Get remaining budget in cents
    pub fn remaining_budget_cents(&self) -> f64 {
        self.budget.remaining_cents()
    }

    /// Get remaining budget in dollars
    pub fn remaining_budget_usd(&self) -> f64 {
        self.budget.remaining_usd()
    }

    /// Check if budget is exceeded
    pub fn is_budget_exceeded(&self) -> bool {
        self.budget.is_exceeded()
    }

    /// Reset budget (starts from zero again)
    pub fn reset_budget(&self) {
        self.budget.reset();
    }
}

#[async_trait]
impl<P: LlmProvider> LlmProvider for BudgetProvider<P> {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        // Check budget before making request
        if self.budget.is_exceeded() {
            return Err(LlmError::ApiError(format!(
                "Budget exceeded: ${:.4} spent of ${:.4} limit",
                self.budget.consumed_cents() / 100.0,
                self.budget.max_budget_cents / 100.0
            )));
        }

        let response = self.inner.complete(request).await?;

        // Track cost after successful response
        if let Some(usage) = &response.usage {
            let cost = self
                .pricing
                .calculate_cost(usage.prompt_tokens, usage.completion_tokens);
            self.budget.consume(cost);

            // Log budget status
            tracing::info!(
                cost_cents = cost,
                remaining_cents = self.budget.remaining_cents(),
                "Request cost tracked against budget"
            );

            // Warn if budget is getting low
            let remaining_pct =
                self.budget.remaining_cents() / self.budget.max_budget_cents * 100.0;
            if remaining_pct < 10.0 && remaining_pct > 0.0 {
                tracing::warn!(
                    remaining_pct = format!("{:.1}%", remaining_pct),
                    "Budget running low"
                );
            }
        }

        Ok(response)
    }
}

#[async_trait]
impl<P: EmbeddingProvider> EmbeddingProvider for BudgetProvider<P> {
    async fn embed(&self, request: EmbeddingRequest) -> Result<EmbeddingResponse> {
        // Check budget before making request
        if self.budget.is_exceeded() {
            return Err(LlmError::ApiError(format!(
                "Budget exceeded: ${:.4} spent of ${:.4} limit",
                self.budget.consumed_cents() / 100.0,
                self.budget.max_budget_cents / 100.0
            )));
        }

        let response = self.inner.embed(request).await?;

        // Track cost after successful response
        if let Some(usage) = &response.usage {
            let cost = self.pricing.calculate_cost(usage.prompt_tokens, 0);
            self.budget.consume(cost);

            tracing::info!(
                cost_cents = cost,
                remaining_cents = self.budget.remaining_cents(),
                "Embedding cost tracked against budget"
            );
        }

        Ok(response)
    }
}
