//! Automatic provider selection based on requirements
//!
//! This module provides intelligent provider selection based on various criteria
//! such as cost, speed, capabilities, and availability.

use crate::{LlmProvider, LlmRequest, LlmResponse, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Selection criteria for choosing an LLM provider
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectionCriteria {
    /// Prioritize cost (lower is better)
    #[serde(default)]
    pub optimize_cost: bool,

    /// Prioritize speed (lower latency)
    #[serde(default)]
    pub optimize_speed: bool,

    /// Require specific capabilities
    #[serde(default)]
    pub requires_function_calling: bool,

    #[serde(default)]
    pub requires_vision: bool,

    #[serde(default)]
    pub requires_streaming: bool,

    /// Maximum acceptable cost per 1M tokens (USD)
    pub max_cost_per_million: Option<f64>,

    /// Preferred providers (tried first)
    #[serde(default)]
    pub preferred_providers: Vec<String>,

    /// Excluded providers (never selected)
    #[serde(default)]
    pub excluded_providers: Vec<String>,
}

/// Provider metadata for selection
#[derive(Debug, Clone)]
pub struct ProviderMetadata {
    pub name: String,
    pub cost_per_million_input: f64,
    pub cost_per_million_output: f64,
    pub typical_latency_ms: u64,
    pub supports_function_calling: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub max_tokens: u32,
}

impl ProviderMetadata {
    /// OpenAI GPT-4 metadata
    pub fn openai_gpt4() -> Self {
        Self {
            name: "openai-gpt4".to_string(),
            cost_per_million_input: 30.0,
            cost_per_million_output: 60.0,
            typical_latency_ms: 2000,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 128000,
        }
    }

    /// OpenAI GPT-4o metadata
    pub fn openai_gpt4o() -> Self {
        Self {
            name: "openai-gpt4o".to_string(),
            cost_per_million_input: 5.0,
            cost_per_million_output: 15.0,
            typical_latency_ms: 1500,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 128000,
        }
    }

    /// OpenAI GPT-4o-mini metadata
    pub fn openai_gpt4o_mini() -> Self {
        Self {
            name: "openai-gpt4o-mini".to_string(),
            cost_per_million_input: 0.15,
            cost_per_million_output: 0.6,
            typical_latency_ms: 800,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 128000,
        }
    }

    /// OpenAI o1-preview metadata (reasoning model)
    pub fn openai_o1_preview() -> Self {
        Self {
            name: "openai-o1-preview".to_string(),
            cost_per_million_input: 15.0,
            cost_per_million_output: 60.0,
            typical_latency_ms: 5000,
            supports_function_calling: false,
            supports_vision: false,
            supports_streaming: false,
            max_tokens: 128000,
        }
    }

    /// OpenAI o1-mini metadata (reasoning model)
    pub fn openai_o1_mini() -> Self {
        Self {
            name: "openai-o1-mini".to_string(),
            cost_per_million_input: 3.0,
            cost_per_million_output: 12.0,
            typical_latency_ms: 3000,
            supports_function_calling: false,
            supports_vision: false,
            supports_streaming: false,
            max_tokens: 128000,
        }
    }

    /// OpenAI GPT-3.5 Turbo metadata
    pub fn openai_gpt35_turbo() -> Self {
        Self {
            name: "openai-gpt35-turbo".to_string(),
            cost_per_million_input: 0.5,
            cost_per_million_output: 1.5,
            typical_latency_ms: 800,
            supports_function_calling: true,
            supports_vision: false,
            supports_streaming: true,
            max_tokens: 16385,
        }
    }

    /// Anthropic Claude 3 Opus metadata
    pub fn anthropic_claude3_opus() -> Self {
        Self {
            name: "anthropic-claude3-opus".to_string(),
            cost_per_million_input: 15.0,
            cost_per_million_output: 75.0,
            typical_latency_ms: 2500,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 200000,
        }
    }

    /// Anthropic Claude 3.5 Sonnet metadata (newer model)
    pub fn anthropic_claude35_sonnet() -> Self {
        Self {
            name: "anthropic-claude35-sonnet".to_string(),
            cost_per_million_input: 3.0,
            cost_per_million_output: 15.0,
            typical_latency_ms: 1200,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 200000,
        }
    }

    /// Anthropic Claude 3 Sonnet metadata (original)
    pub fn anthropic_claude3_sonnet() -> Self {
        Self {
            name: "anthropic-claude3-sonnet".to_string(),
            cost_per_million_input: 3.0,
            cost_per_million_output: 15.0,
            typical_latency_ms: 1500,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 200000,
        }
    }

    /// Anthropic Claude 3.5 Haiku metadata (newer model)
    pub fn anthropic_claude35_haiku() -> Self {
        Self {
            name: "anthropic-claude35-haiku".to_string(),
            cost_per_million_input: 0.8,
            cost_per_million_output: 4.0,
            typical_latency_ms: 400,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 200000,
        }
    }

    /// Anthropic Claude 3 Haiku metadata (original)
    pub fn anthropic_claude3_haiku() -> Self {
        Self {
            name: "anthropic-claude3-haiku".to_string(),
            cost_per_million_input: 0.25,
            cost_per_million_output: 1.25,
            typical_latency_ms: 500,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 200000,
        }
    }

    /// Google Gemini Pro metadata
    pub fn google_gemini_pro() -> Self {
        Self {
            name: "google-gemini-pro".to_string(),
            cost_per_million_input: 0.5,
            cost_per_million_output: 1.5,
            typical_latency_ms: 1200,
            supports_function_calling: true,
            supports_vision: false,
            supports_streaming: true,
            max_tokens: 32768,
        }
    }

    /// Google Gemini Flash metadata (faster, cheaper)
    pub fn google_gemini_flash() -> Self {
        Self {
            name: "google-gemini-flash".to_string(),
            cost_per_million_input: 0.375,
            cost_per_million_output: 1.125,
            typical_latency_ms: 800,
            supports_function_calling: true,
            supports_vision: true,
            supports_streaming: true,
            max_tokens: 1048576,
        }
    }

    /// Ollama (local) metadata
    pub fn ollama_local() -> Self {
        Self {
            name: "ollama-local".to_string(),
            cost_per_million_input: 0.0,
            cost_per_million_output: 0.0,
            typical_latency_ms: 5000,
            supports_function_calling: false,
            supports_vision: false,
            supports_streaming: true,
            max_tokens: 4096,
        }
    }

    /// Calculate score based on criteria (higher is better)
    fn calculate_score(&self, criteria: &SelectionCriteria) -> f64 {
        let mut score = 100.0;

        // Check hard requirements
        if criteria.requires_function_calling && !self.supports_function_calling {
            return 0.0;
        }
        if criteria.requires_vision && !self.supports_vision {
            return 0.0;
        }
        if criteria.requires_streaming && !self.supports_streaming {
            return 0.0;
        }

        // Check cost limit
        if let Some(max_cost) = criteria.max_cost_per_million {
            if self.cost_per_million_input > max_cost || self.cost_per_million_output > max_cost {
                return 0.0;
            }
        }

        // Optimize for cost (inverse relationship)
        if criteria.optimize_cost {
            let avg_cost = (self.cost_per_million_input + self.cost_per_million_output) / 2.0;
            // Normalize cost (assuming max reasonable cost is $100/M)
            let cost_score = (100.0 - avg_cost.min(100.0)) / 100.0 * 50.0;
            score += cost_score;
        }

        // Optimize for speed (inverse relationship)
        if criteria.optimize_speed {
            // Normalize latency (assuming max reasonable latency is 5000ms)
            let speed_score = (5000.0 - self.typical_latency_ms as f64).max(0.0) / 5000.0 * 50.0;
            score += speed_score;
        }

        // Preferred providers get a boost
        if criteria.preferred_providers.contains(&self.name) {
            score += 100.0;
        }

        score
    }
}

/// Registered provider with metadata
struct RegisteredProvider {
    metadata: ProviderMetadata,
    provider: Arc<dyn LlmProvider>,
}

/// Smart provider selector with automatic fallback
pub struct ProviderSelector {
    providers: Vec<RegisteredProvider>,
}

impl ProviderSelector {
    /// Create a new provider selector
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a provider with metadata
    pub fn register(
        &mut self,
        metadata: ProviderMetadata,
        provider: Arc<dyn LlmProvider>,
    ) -> &mut Self {
        self.providers
            .push(RegisteredProvider { metadata, provider });
        self
    }

    /// Select the best provider based on criteria
    pub fn select(&self, criteria: &SelectionCriteria) -> Option<Arc<dyn LlmProvider>> {
        let mut candidates: Vec<_> = self
            .providers
            .iter()
            .filter(|p| !criteria.excluded_providers.contains(&p.metadata.name))
            .map(|p| {
                let score = p.metadata.calculate_score(criteria);
                (score, p)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        // Sort by score (descending)
        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        candidates.first().map(|(_, p)| Arc::clone(&p.provider))
    }

    /// Complete a request using automatic provider selection
    pub async fn complete_with_criteria(
        &self,
        request: LlmRequest,
        criteria: SelectionCriteria,
    ) -> Result<LlmResponse> {
        let provider = self.select(&criteria).ok_or_else(|| {
            crate::LlmError::ConfigError("No suitable provider found".to_string())
        })?;

        provider.complete(request).await
    }

    /// Complete a request with automatic fallback on failure
    pub async fn complete_with_fallback(&self, request: LlmRequest) -> Result<LlmResponse> {
        let mut last_error = None;

        // Try each provider in order of score (using default criteria)
        let criteria = SelectionCriteria::default();
        let mut candidates: Vec<_> = self
            .providers
            .iter()
            .map(|p| {
                let score = p.metadata.calculate_score(&criteria);
                (score, &p.provider)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        candidates.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        for (_, provider) in candidates {
            match provider.complete(request.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!("Provider failed, trying next: {:?}", e);
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| crate::LlmError::ConfigError("No providers available".to_string())))
    }

    /// List all registered providers
    pub fn list_providers(&self) -> Vec<&ProviderMetadata> {
        self.providers.iter().map(|p| &p.metadata).collect()
    }
}

impl Default for ProviderSelector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for ProviderSelector {
    async fn complete(&self, request: LlmRequest) -> Result<LlmResponse> {
        self.complete_with_fallback(request).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_metadata_scoring() {
        let gpt4 = ProviderMetadata::openai_gpt4();
        let gpt35 = ProviderMetadata::openai_gpt35_turbo();
        let haiku = ProviderMetadata::anthropic_claude3_haiku();

        // Cost optimization should favor cheaper models
        let cost_criteria = SelectionCriteria {
            optimize_cost: true,
            ..Default::default()
        };

        let gpt4_score = gpt4.calculate_score(&cost_criteria);
        let gpt35_score = gpt35.calculate_score(&cost_criteria);
        let haiku_score = haiku.calculate_score(&cost_criteria);

        assert!(haiku_score > gpt35_score);
        assert!(gpt35_score > gpt4_score);
    }

    #[test]
    fn test_provider_metadata_speed() {
        let gpt4 = ProviderMetadata::openai_gpt4();
        let haiku = ProviderMetadata::anthropic_claude3_haiku();

        // Speed optimization should favor faster models
        let speed_criteria = SelectionCriteria {
            optimize_speed: true,
            ..Default::default()
        };

        let gpt4_score = gpt4.calculate_score(&speed_criteria);
        let haiku_score = haiku.calculate_score(&speed_criteria);

        assert!(haiku_score > gpt4_score);
    }

    #[test]
    fn test_provider_metadata_capabilities() {
        let gpt35 = ProviderMetadata::openai_gpt35_turbo();
        let ollama = ProviderMetadata::ollama_local();

        // Function calling requirement
        let func_criteria = SelectionCriteria {
            requires_function_calling: true,
            ..Default::default()
        };

        let gpt35_score = gpt35.calculate_score(&func_criteria);
        let ollama_score = ollama.calculate_score(&func_criteria);

        assert!(gpt35_score > 0.0);
        assert_eq!(ollama_score, 0.0); // Ollama doesn't support function calling
    }

    #[test]
    fn test_provider_metadata_cost_limit() {
        let gpt4 = ProviderMetadata::openai_gpt4();

        let cost_limit_criteria = SelectionCriteria {
            max_cost_per_million: Some(10.0),
            ..Default::default()
        };

        let score = gpt4.calculate_score(&cost_limit_criteria);
        assert_eq!(score, 0.0); // GPT-4 is too expensive
    }

    #[test]
    fn test_preferred_providers() {
        let haiku = ProviderMetadata::anthropic_claude3_haiku();

        let preferred_criteria = SelectionCriteria {
            preferred_providers: vec!["anthropic-claude3-haiku".to_string()],
            ..Default::default()
        };

        let score = haiku.calculate_score(&preferred_criteria);
        assert!(score > 100.0); // Gets bonus for being preferred
    }

    #[test]
    fn test_selection_criteria_default() {
        let criteria = SelectionCriteria::default();
        assert!(!criteria.optimize_cost);
        assert!(!criteria.optimize_speed);
        assert!(!criteria.requires_function_calling);
        assert!(criteria.preferred_providers.is_empty());
    }
}
