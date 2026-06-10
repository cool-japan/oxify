//! Model recommendation system
//!
//! This module helps users choose the right LLM model based on their requirements,
//! balancing cost, speed, capabilities, and quality.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Use case categories for model selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UseCase {
    /// Simple text generation and basic queries
    SimpleGeneration,
    /// Code generation and technical tasks
    CodeGeneration,
    /// Complex reasoning and analysis
    ComplexReasoning,
    /// Long-form content creation
    ContentCreation,
    /// Real-time chat applications
    RealtimeChat,
    /// Data extraction and structured output
    DataExtraction,
    /// Translation tasks
    Translation,
    /// Summarization tasks
    Summarization,
    /// Vision tasks (image understanding)
    Vision,
    /// Function calling and tool use
    FunctionCalling,
    /// Embedding generation
    Embeddings,
}

/// Priority for model selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationGoal {
    /// Minimize cost above all else
    MinimizeCost,
    /// Minimize latency for real-time applications
    MinimizeLatency,
    /// Balance cost and performance
    Balanced,
    /// Maximize quality regardless of cost
    MaximizeQuality,
}

/// Budget constraint for model selection
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BudgetConstraint {
    /// No budget constraint
    Unlimited,
    /// Maximum cost per request in USD cents
    MaxCostPerRequest(f64),
    /// Maximum cost per 1M tokens in USD
    MaxCostPerMillion(f64),
}

/// Model recommendation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationRequest {
    /// Primary use case
    pub use_case: UseCase,
    /// Optimization goal
    pub goal: OptimizationGoal,
    /// Budget constraint
    pub budget: BudgetConstraint,
    /// Estimated prompt length in tokens
    pub estimated_prompt_tokens: Option<u32>,
    /// Estimated completion length in tokens
    pub estimated_completion_tokens: Option<u32>,
    /// Whether streaming is required
    pub requires_streaming: bool,
}

/// Model recommendation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRecommendation {
    /// Recommended model name
    pub model: String,
    /// Provider name
    pub provider: String,
    /// Confidence score (0-100)
    pub confidence: u8,
    /// Estimated cost in USD cents
    pub estimated_cost: Option<f64>,
    /// Estimated latency in milliseconds
    pub estimated_latency_ms: u64,
    /// Reason for recommendation
    pub reason: String,
    /// Alternative models (if any)
    pub alternatives: Vec<AlternativeModel>,
}

/// Alternative model suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlternativeModel {
    /// Model name
    pub model: String,
    /// Provider name
    pub provider: String,
    /// Why this might be a good alternative
    pub reason: String,
}

/// Model recommender
pub struct ModelRecommender {
    models: HashMap<String, ModelInfo>,
}

#[derive(Debug, Clone)]
struct ModelInfo {
    provider: String,
    cost_per_1k_input: f64,
    cost_per_1k_output: f64,
    latency_ms: u64,
    quality_score: u8,
    supports_streaming: bool,
    supports_vision: bool,
    supports_functions: bool,
    use_cases: Vec<UseCase>,
}

impl Default for ModelRecommender {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRecommender {
    /// Create a new model recommender with default models
    pub fn new() -> Self {
        let mut models = HashMap::new();

        // OpenAI models
        models.insert(
            "gpt-4o".to_string(),
            ModelInfo {
                provider: "openai".to_string(),
                cost_per_1k_input: 0.5,
                cost_per_1k_output: 1.5,
                latency_ms: 1500,
                quality_score: 95,
                supports_streaming: true,
                supports_vision: true,
                supports_functions: true,
                use_cases: vec![
                    UseCase::ComplexReasoning,
                    UseCase::CodeGeneration,
                    UseCase::ContentCreation,
                    UseCase::Vision,
                    UseCase::FunctionCalling,
                ],
            },
        );

        models.insert(
            "gpt-4o-mini".to_string(),
            ModelInfo {
                provider: "openai".to_string(),
                cost_per_1k_input: 0.015,
                cost_per_1k_output: 0.06,
                latency_ms: 800,
                quality_score: 80,
                supports_streaming: true,
                supports_vision: true,
                supports_functions: true,
                use_cases: vec![
                    UseCase::SimpleGeneration,
                    UseCase::RealtimeChat,
                    UseCase::DataExtraction,
                    UseCase::Summarization,
                ],
            },
        );

        models.insert(
            "gpt-3.5-turbo".to_string(),
            ModelInfo {
                provider: "openai".to_string(),
                cost_per_1k_input: 0.05,
                cost_per_1k_output: 0.15,
                latency_ms: 800,
                quality_score: 70,
                supports_streaming: true,
                supports_vision: false,
                supports_functions: true,
                use_cases: vec![
                    UseCase::SimpleGeneration,
                    UseCase::RealtimeChat,
                    UseCase::Translation,
                ],
            },
        );

        // Anthropic models
        models.insert(
            "claude-3-5-sonnet".to_string(),
            ModelInfo {
                provider: "anthropic".to_string(),
                cost_per_1k_input: 0.3,
                cost_per_1k_output: 1.5,
                latency_ms: 1200,
                quality_score: 98,
                supports_streaming: true,
                supports_vision: true,
                supports_functions: true,
                use_cases: vec![
                    UseCase::ComplexReasoning,
                    UseCase::CodeGeneration,
                    UseCase::ContentCreation,
                    UseCase::Vision,
                    UseCase::FunctionCalling,
                ],
            },
        );

        models.insert(
            "claude-3-haiku".to_string(),
            ModelInfo {
                provider: "anthropic".to_string(),
                cost_per_1k_input: 0.025,
                cost_per_1k_output: 0.125,
                latency_ms: 500,
                quality_score: 75,
                supports_streaming: true,
                supports_vision: true,
                supports_functions: true,
                use_cases: vec![
                    UseCase::SimpleGeneration,
                    UseCase::RealtimeChat,
                    UseCase::DataExtraction,
                ],
            },
        );

        // Google models
        models.insert(
            "gemini-1.5-flash".to_string(),
            ModelInfo {
                provider: "google".to_string(),
                cost_per_1k_input: 0.00375,
                cost_per_1k_output: 0.01125,
                latency_ms: 800,
                quality_score: 78,
                supports_streaming: true,
                supports_vision: true,
                supports_functions: true,
                use_cases: vec![
                    UseCase::SimpleGeneration,
                    UseCase::RealtimeChat,
                    UseCase::Vision,
                ],
            },
        );

        Self { models }
    }

    /// Get a model recommendation based on requirements
    pub fn recommend(&self, request: &RecommendationRequest) -> Option<ModelRecommendation> {
        let mut candidates: Vec<(&String, &ModelInfo, f64)> = self
            .models
            .iter()
            .filter(|(_, info)| {
                // Filter by use case
                if !info.use_cases.contains(&request.use_case) {
                    return false;
                }

                // Filter by streaming requirement
                if request.requires_streaming && !info.supports_streaming {
                    return false;
                }

                // Filter by budget
                if let (Some(prompt_tokens), Some(completion_tokens)) = (
                    request.estimated_prompt_tokens,
                    request.estimated_completion_tokens,
                ) {
                    let cost = (prompt_tokens as f64 / 1000.0) * info.cost_per_1k_input
                        + (completion_tokens as f64 / 1000.0) * info.cost_per_1k_output;

                    match request.budget {
                        BudgetConstraint::MaxCostPerRequest(max_cost) => {
                            if cost > max_cost {
                                return false;
                            }
                        }
                        BudgetConstraint::MaxCostPerMillion(max_per_million) => {
                            let avg_cost_per_1k =
                                (info.cost_per_1k_input + info.cost_per_1k_output) / 2.0;
                            if avg_cost_per_1k * 1000.0 > max_per_million {
                                return false;
                            }
                        }
                        BudgetConstraint::Unlimited => {}
                    }
                }

                true
            })
            .map(|(name, info)| {
                let score = self.calculate_score(info, &request.goal);
                (name, info, score)
            })
            .collect();

        // Sort by score (highest first)
        candidates.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Get the best recommendation
        let best = candidates.first()?;
        let (model_name, model_info, score) = best;

        let estimated_cost = if let (Some(prompt_tokens), Some(completion_tokens)) = (
            request.estimated_prompt_tokens,
            request.estimated_completion_tokens,
        ) {
            let cost = (prompt_tokens as f64 / 1000.0) * model_info.cost_per_1k_input
                + (completion_tokens as f64 / 1000.0) * model_info.cost_per_1k_output;
            Some(cost)
        } else {
            None
        };

        let reason = self.generate_reason(model_info, &request.goal, &request.use_case);

        // Get alternatives
        let alternatives: Vec<AlternativeModel> = candidates
            .iter()
            .skip(1)
            .take(2)
            .map(|(alt_name, alt_info, _)| AlternativeModel {
                model: (*alt_name).clone(),
                provider: alt_info.provider.clone(),
                reason: format!(
                    "Alternative with different cost/performance tradeoff (latency: {}ms)",
                    alt_info.latency_ms
                ),
            })
            .collect();

        Some(ModelRecommendation {
            model: (*model_name).clone(),
            provider: model_info.provider.clone(),
            confidence: score.clamp(0.0, 100.0) as u8,
            estimated_cost,
            estimated_latency_ms: model_info.latency_ms,
            reason,
            alternatives,
        })
    }

    fn calculate_score(&self, info: &ModelInfo, goal: &OptimizationGoal) -> f64 {
        match goal {
            OptimizationGoal::MinimizeCost => {
                let avg_cost = (info.cost_per_1k_input + info.cost_per_1k_output) / 2.0;
                // Lower cost = higher score
                100.0 - (avg_cost.min(10.0) * 10.0)
            }
            OptimizationGoal::MinimizeLatency => {
                // Lower latency = higher score
                100.0 - (info.latency_ms as f64 / 50.0).min(100.0)
            }
            OptimizationGoal::Balanced => {
                let cost_score = {
                    let avg_cost = (info.cost_per_1k_input + info.cost_per_1k_output) / 2.0;
                    100.0 - (avg_cost.min(10.0) * 10.0)
                };
                let latency_score = 100.0 - (info.latency_ms as f64 / 50.0).min(100.0);
                let quality_score = info.quality_score as f64;

                // Weighted average
                cost_score * 0.3 + latency_score * 0.3 + quality_score * 0.4
            }
            OptimizationGoal::MaximizeQuality => info.quality_score as f64,
        }
    }

    fn generate_reason(
        &self,
        info: &ModelInfo,
        goal: &OptimizationGoal,
        use_case: &UseCase,
    ) -> String {
        let mut reason = format!("Best match for {:?} use case. ", use_case);

        match goal {
            OptimizationGoal::MinimizeCost => {
                reason.push_str(&format!(
                    "Very cost-effective at ${:.4}/1K tokens (avg). ",
                    (info.cost_per_1k_input + info.cost_per_1k_output) / 2.0
                ));
            }
            OptimizationGoal::MinimizeLatency => {
                reason.push_str(&format!("Fast response time (~{}ms). ", info.latency_ms));
            }
            OptimizationGoal::Balanced => {
                reason.push_str("Good balance of cost, speed, and quality. ");
            }
            OptimizationGoal::MaximizeQuality => {
                reason.push_str(&format!(
                    "Highest quality output (score: {}). ",
                    info.quality_score
                ));
            }
        }

        if info.supports_vision {
            reason.push_str("Supports vision. ");
        }
        if info.supports_functions {
            reason.push_str("Supports function calling. ");
        }

        reason
    }

    /// Get all available models for a use case
    pub fn list_models_for_use_case(&self, use_case: UseCase) -> Vec<String> {
        self.models
            .iter()
            .filter(|(_, info)| info.use_cases.contains(&use_case))
            .map(|(name, _)| name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recommend_cheap_model() {
        let recommender = ModelRecommender::new();
        let request = RecommendationRequest {
            use_case: UseCase::SimpleGeneration,
            goal: OptimizationGoal::MinimizeCost,
            budget: BudgetConstraint::Unlimited,
            estimated_prompt_tokens: Some(100),
            estimated_completion_tokens: Some(50),
            requires_streaming: false,
        };

        let recommendation = recommender.recommend(&request);
        assert!(recommendation.is_some());

        let rec = recommendation.unwrap();
        assert!(rec.estimated_cost.is_some());
        assert!(rec.confidence > 0);
    }

    #[test]
    fn test_recommend_fast_model() {
        let recommender = ModelRecommender::new();
        let request = RecommendationRequest {
            use_case: UseCase::RealtimeChat,
            goal: OptimizationGoal::MinimizeLatency,
            budget: BudgetConstraint::Unlimited,
            estimated_prompt_tokens: Some(100),
            estimated_completion_tokens: Some(50),
            requires_streaming: true,
        };

        let recommendation = recommender.recommend(&request);
        assert!(recommendation.is_some());

        let rec = recommendation.unwrap();
        assert!(rec.estimated_latency_ms < 2000);
    }

    #[test]
    fn test_recommend_with_budget() {
        let recommender = ModelRecommender::new();
        let request = RecommendationRequest {
            use_case: UseCase::SimpleGeneration,
            goal: OptimizationGoal::Balanced,
            budget: BudgetConstraint::MaxCostPerRequest(0.01),
            estimated_prompt_tokens: Some(100),
            estimated_completion_tokens: Some(100),
            requires_streaming: false,
        };

        let recommendation = recommender.recommend(&request);
        assert!(recommendation.is_some());

        let rec = recommendation.unwrap();
        assert!(rec.estimated_cost.unwrap() <= 0.01);
    }

    #[test]
    fn test_list_models_for_use_case() {
        let recommender = ModelRecommender::new();
        let models = recommender.list_models_for_use_case(UseCase::Vision);
        assert!(!models.is_empty());
    }

    #[test]
    fn test_recommend_balanced() {
        let recommender = ModelRecommender::new();
        let request = RecommendationRequest {
            use_case: UseCase::ComplexReasoning,
            goal: OptimizationGoal::Balanced,
            budget: BudgetConstraint::Unlimited,
            estimated_prompt_tokens: Some(500),
            estimated_completion_tokens: Some(500),
            requires_streaming: false,
        };

        let recommendation = recommender.recommend(&request);
        assert!(recommendation.is_some());
    }
}
