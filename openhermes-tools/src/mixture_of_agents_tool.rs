//! Mixture of Agents (MoA) tool — multi-model reasoning.
//!
//! 2-layer architecture:
//! 1. Reference layer: Multiple models generate diverse responses in parallel
//! 2. Aggregation layer: A single model synthesizes the best answer
//!
//! Uses auxiliary_client for all LLM calls.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::task::JoinSet;
use tracing::{debug, info, warn};

use crate::registry::Tool;

/// Default reference models.
const DEFAULT_REFERENCE_MODELS: &[&str] = &[
    "anthropic/claude-sonnet-4",
    "openai/gpt-4.1",
    "google/gemini-2.5-pro",
    "deepseek/deepseek-chat",
];

/// Default aggregator model.
const DEFAULT_AGGREGATOR_MODEL: &str = "anthropic/claude-sonnet-4";

/// Temperature for reference models.
const REFERENCE_TEMPERATURE: f64 = 0.6;

/// Temperature for aggregator.
const AGGREGATOR_TEMPERATURE: f64 = 0.4;

/// Maximum retries per reference model.
const MAX_RETRIES: usize = 2;

/// Minimum successful references required.
const MIN_SUCCESSFUL_REFS: usize = 1;

/// MoA tool.
pub struct MixtureOfAgentsTool;

#[async_trait]
impl Tool for MixtureOfAgentsTool {
    fn name(&self) -> &str {
        "mixture_of_agents"
    }

    fn toolset(&self) -> &str {
        "moa"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": "mixture_of_agents",
            "description": "Use Mixture of Agents (MoA) for complex reasoning. Multiple models generate diverse responses, then an aggregator synthesizes the best answer. Use for important decisions, complex analysis, or when you want high-quality output.",
            "parameters": {
                "type": "object",
                "properties": {
                    "prompt": {
                        "type": "string",
                        "description": "The prompt/question to send to all models"
                    },
                    "reference_models": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Models to use as references (default: claude, gpt-4.1, gemini-pro, deepseek)"
                    },
                    "aggregator_model": {
                        "type": "string",
                        "description": "Model to use for aggregation (default: claude-sonnet-4)"
                    }
                },
                "required": ["prompt"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<String> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: prompt"))?;

        let ref_models: Vec<String> = if let Some(arr) = args["reference_models"].as_array() {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        } else {
            DEFAULT_REFERENCE_MODELS.iter().map(|s| s.to_string()).collect()
        };

        let aggregator = args["aggregator_model"]
            .as_str()
            .unwrap_or(DEFAULT_AGGREGATOR_MODEL);

        info!(
            ref_count = ref_models.len(),
            aggregator = aggregator,
            "Starting Mixture of Agents"
        );

        // Layer 1: Reference models in parallel
        let references = run_reference_layer(prompt, &ref_models).await;

        if references.is_empty() {
            return Ok(serde_json::json!({
                "error": "All reference models failed. Cannot aggregate.",
                "success": false,
            }).to_string());
        }

        if references.len() < MIN_SUCCESSFUL_REFS {
            warn!(
                successful = references.len(),
                required = MIN_SUCCESSFUL_REFS,
                "Insufficient reference responses"
            );
        }

        info!(
            successful = references.len(),
            total = ref_models.len(),
            "Reference layer complete"
        );

        // Layer 2: Aggregation
        let aggregation_prompt = build_aggregation_prompt(prompt, &references);
        let aggregated = crate::llm_client::call_llm_with_model(
            &aggregation_prompt,
            aggregator,
            Some(4096),
            Some(AGGREGATOR_TEMPERATURE),
        )
        .await;

        match aggregated {
            Ok(result) => Ok(serde_json::json!({
                "success": true,
                "result": result,
                "reference_count": references.len(),
                "aggregator": aggregator,
                "models_used": references.iter().map(|r| r.model.as_str()).collect::<Vec<_>>(),
            }).to_string()),
            Err(e) => {
                warn!(error = %e, "Aggregation failed, returning best reference");
                // Fall back to the longest reference response
                let best = references
                    .iter()
                    .max_by_key(|r| r.response.len())
                    .map(|r| r.response.as_str())
                    .unwrap_or("All models failed.");

                Ok(serde_json::json!({
                    "success": true,
                    "result": best,
                    "reference_count": references.len(),
                    "aggregator": "fallback (best reference)",
                    "note": format!("Aggregation failed: {}. Using best reference response.", e),
                }).to_string())
            }
        }
    }
}

/// A successful reference response.
struct ReferenceResponse {
    model: String,
    response: String,
}

/// Run reference models in parallel with retries.
async fn run_reference_layer(prompt: &str, models: &[String]) -> Vec<ReferenceResponse> {
    let mut join_set = JoinSet::new();

    for model in models {
        let model = model.clone();
        let prompt = prompt.to_string();

        join_set.spawn(async move {
            for attempt in 0..MAX_RETRIES {
                match crate::llm_client::call_llm_with_model(
                    &prompt,
                    &model,
                    Some(4096),
                    Some(REFERENCE_TEMPERATURE),
                )
                .await
                {
                    Ok(response) if !response.trim().is_empty() => {
                        debug!(model = %model, "Reference response received");
                        return Some(ReferenceResponse { model, response });
                    }
                    Ok(_) => {
                        debug!(model = %model, attempt = attempt, "Empty response, retrying");
                    }
                    Err(e) => {
                        debug!(model = %model, attempt = attempt, error = %e, "Reference call failed");
                        if attempt < MAX_RETRIES - 1 {
                            let backoff = std::time::Duration::from_millis(
                                500 * (2_u64.pow(attempt as u32)),
                            );
                            tokio::time::sleep(backoff).await;
                        }
                    }
                }
            }
            warn!(model = %model, "All retries exhausted");
            None
        });
    }

    let mut results = Vec::new();
    while let Some(join_result) = join_set.join_next().await {
        if let Ok(Some(response)) = join_result {
            results.push(response);
        }
    }

    results
}

/// Build the aggregation prompt with all reference responses.
fn build_aggregation_prompt(original_prompt: &str, references: &[ReferenceResponse]) -> String {
    let mut parts = vec![
        "You are an expert aggregator. Multiple AI models have responded to the same prompt.".to_string(),
        "Your job is to synthesize the best possible answer by:".to_string(),
        "1. Identifying the strongest insights from each response".to_string(),
        "2. Resolving any contradictions using your best judgment".to_string(),
        "3. Producing a clear, comprehensive, well-structured final answer".to_string(),
        String::new(),
        format!("## Original Prompt\n{}", original_prompt),
        String::new(),
        "## Reference Responses".to_string(),
    ];

    for (i, r) in references.iter().enumerate() {
        parts.push(format!(
            "### Response {} ({})\n{}",
            i + 1,
            r.model,
            r.response
        ));
        parts.push(String::new());
    }

    parts.push("## Your Synthesized Answer".to_string());
    parts.push("Provide the best possible answer, combining the strongest elements from all responses:".to_string());

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregation_prompt_format() {
        let refs = vec![
            ReferenceResponse {
                model: "model-a".to_string(),
                response: "Answer A".to_string(),
            },
            ReferenceResponse {
                model: "model-b".to_string(),
                response: "Answer B".to_string(),
            },
        ];
        let prompt = build_aggregation_prompt("What is Rust?", &refs);
        assert!(prompt.contains("What is Rust?"));
        assert!(prompt.contains("model-a"));
        assert!(prompt.contains("Answer A"));
        assert!(prompt.contains("model-b"));
        assert!(prompt.contains("Answer B"));
    }

    #[test]
    fn test_tool_name() {
        let tool = MixtureOfAgentsTool;
        assert_eq!(tool.name(), "mixture_of_agents");
        assert_eq!(tool.toolset(), "moa");
    }

    #[test]
    fn test_default_models() {
        assert!(DEFAULT_REFERENCE_MODELS.len() >= 3);
        assert!(!DEFAULT_AGGREGATOR_MODEL.is_empty());
    }
}
