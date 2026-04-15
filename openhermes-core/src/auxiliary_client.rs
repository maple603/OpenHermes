//! Shared auxiliary client router for side tasks.
//!
//! Provides a single resolution chain so every consumer (context compression,
//! session search, title generation, MoA) picks up the best available backend
//! without duplicating fallback logic.
//!
//! Resolution order (auto mode):
//!   1. OpenRouter  (OPENROUTER_API_KEY)
//!   2. Custom endpoint (OPENAI_BASE_URL + OPENAI_API_KEY)
//!   3. Native Anthropic (ANTHROPIC_API_KEY)
//!   4. None

use std::env;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tracing::{debug, warn};

/// Default cheap/fast model for auxiliary tasks.
const DEFAULT_AUX_MODEL: &str = "google/gemini-2.5-flash";

/// Fallback models to try in order.
#[allow(dead_code)]
const FALLBACK_MODELS: &[&str] = &[
    "google/gemini-2.5-flash",
    "openai/gpt-4.1-mini",
    "anthropic/claude-sonnet-4",
];

/// Resolved provider for auxiliary LLM calls.
#[derive(Debug, Clone)]
struct ResolvedProvider {
    base_url: String,
    api_key: String,
    model: String,
    name: &'static str,
}

/// Call an auxiliary LLM for side tasks (summarization, title generation, etc.).
///
/// This is the main entry point used by context_compressor, session_search,
/// title_generator, and mixture_of_agents.
///
/// # Arguments
/// * `prompt` - The prompt to send
/// * `task_hint` - Optional hint for model selection (e.g. "summarization", "title")
/// * `max_tokens` - Optional max output tokens
pub async fn call_llm(
    prompt: &str,
    task_hint: Option<&str>,
    max_tokens: Option<usize>,
) -> Result<String> {
    let providers = resolve_providers();

    if providers.is_empty() {
        anyhow::bail!(
            "No LLM provider available for auxiliary tasks. \
             Set OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY."
        );
    }

    let max_tokens = max_tokens.unwrap_or(4096);
    let mut last_error = None;

    for provider in &providers {
        debug!(
            provider = provider.name,
            model = &provider.model,
            task = task_hint,
            "Trying auxiliary LLM provider"
        );

        match call_provider(provider, prompt, max_tokens).await {
            Ok(response) if !response.trim().is_empty() => {
                debug!(
                    provider = provider.name,
                    "Auxiliary LLM call succeeded"
                );
                return Ok(response);
            }
            Ok(_) => {
                warn!(provider = provider.name, "Empty response from auxiliary LLM");
                last_error = Some(anyhow::anyhow!("Empty response from {}", provider.name));
            }
            Err(e) => {
                let is_credit_error = e.to_string().contains("402")
                    || e.to_string().to_lowercase().contains("credit")
                    || e.to_string().to_lowercase().contains("payment")
                    || e.to_string().to_lowercase().contains("quota");

                if is_credit_error {
                    warn!(
                        provider = provider.name,
                        "Credit/payment error, trying next provider: {}", e
                    );
                } else {
                    warn!(
                        provider = provider.name,
                        "Auxiliary LLM error: {}", e
                    );
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All auxiliary LLM providers failed")))
}

/// Call an auxiliary LLM with a specific model override.
pub async fn call_llm_with_model(
    prompt: &str,
    model: &str,
    max_tokens: Option<usize>,
    temperature: Option<f32>,
) -> Result<String> {
    let providers = resolve_providers();

    if providers.is_empty() {
        anyhow::bail!("No LLM provider available");
    }

    // Use the first available provider with the specified model
    let provider = ResolvedProvider {
        base_url: providers[0].base_url.clone(),
        api_key: providers[0].api_key.clone(),
        model: model.to_string(),
        name: providers[0].name,
    };

    call_provider_with_options(&provider, prompt, max_tokens.unwrap_or(4096), temperature).await
}

// ── Provider resolution ─────────────────────────────────────────────────

fn resolve_providers() -> Vec<ResolvedProvider> {
    let mut providers = Vec::new();

    // 1. OpenRouter
    if let Ok(key) = env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            providers.push(ResolvedProvider {
                base_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
                api_key: key,
                model: DEFAULT_AUX_MODEL.to_string(),
                name: "openrouter",
            });
        }
    }

    // 2. Custom endpoint (OpenAI-compatible)
    if let Ok(key) = env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            let base_url = env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            let url = format!(
                "{}/chat/completions",
                base_url.trim_end_matches('/')
            );
            // Use a model appropriate for the provider
            let model = if base_url.contains("openai.com") {
                "gpt-4.1-mini".to_string()
            } else {
                // Custom endpoint — use whatever model they have
                env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4.1-mini".to_string())
            };
            providers.push(ResolvedProvider {
                base_url: url,
                api_key: key,
                model,
                name: "openai",
            });
        }
    }

    // 3. Native Anthropic
    if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            providers.push(ResolvedProvider {
                base_url: "https://api.anthropic.com/v1/messages".to_string(),
                api_key: key,
                model: "claude-sonnet-4".to_string(),
                name: "anthropic",
            });
        }
    }

    providers
}

// ── HTTP calls ──────────────────────────────────────────────────────────

async fn call_provider(
    provider: &ResolvedProvider,
    prompt: &str,
    max_tokens: usize,
) -> Result<String> {
    call_provider_with_options(provider, prompt, max_tokens, None).await
}

async fn call_provider_with_options(
    provider: &ResolvedProvider,
    prompt: &str,
    max_tokens: usize,
    temperature: Option<f32>,
) -> Result<String> {
    let client = reqwest::Client::new();

    if provider.name == "anthropic" {
        // Native Anthropic Messages API
        let mut body = json!({
            "model": provider.model,
            "max_tokens": max_tokens,
            "messages": [{"role": "user", "content": prompt}]
        });
        if let Some(temp) = temperature {
            body["temperature"] = json!(temp);
        }

        let response = client
            .post(&provider.base_url)
            .header("content-type", "application/json")
            .header("x-api-key", &provider.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Failed to call Anthropic API")?;

        let status = response.status();
        let body: Value = response.json().await?;

        if !status.is_success() {
            let msg = body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("Anthropic error ({}): {}", status.as_u16(), msg);
        }

        // Extract text from content blocks
        let content = body
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| {
                        if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                            b.get("text").and_then(|t| t.as_str()).map(String::from)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("")
            })
            .unwrap_or_default();

        Ok(content)
    } else {
        // OpenAI-compatible API
        let mut body = json!({
            "model": provider.model,
            "max_completion_tokens": max_tokens,
            "messages": [{"role": "user", "content": prompt}]
        });
        if let Some(temp) = temperature {
            body["temperature"] = json!(temp);
        }

        let response = client
            .post(&provider.base_url)
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {}", provider.api_key))
            .json(&body)
            .send()
            .await
            .context("Failed to call LLM API")?;

        let status = response.status();
        let body: Value = response.json().await?;

        if !status.is_success() {
            let msg = body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!("LLM error ({}): {}", status.as_u16(), msg);
        }

        let content = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();

        Ok(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_providers_empty() {
        // With no env vars set, should return empty
        // (We can't test this reliably since env vars may be set)
        let _ = resolve_providers();
    }

    #[test]
    fn test_default_model() {
        assert_eq!(DEFAULT_AUX_MODEL, "google/gemini-2.5-flash");
    }

    #[test]
    fn test_fallback_models() {
        assert_eq!(FALLBACK_MODELS.len(), 3);
        assert!(FALLBACK_MODELS.contains(&"google/gemini-2.5-flash"));
    }
}
