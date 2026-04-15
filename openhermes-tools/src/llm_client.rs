//! Lightweight LLM client for tools that need auxiliary LLM calls.
//!
//! This is a standalone implementation that doesn't depend on openhermes-core,
//! avoiding circular dependency issues (core depends on tools).
//! Uses the same resolution chain: OpenRouter -> OpenAI -> Anthropic.

use std::env;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tracing::{debug, warn};

/// Default cheap model for auxiliary tasks.
const DEFAULT_AUX_MODEL: &str = "google/gemini-2.5-flash";

/// Call an auxiliary LLM for side tasks within tools.
///
/// Resolution chain: OpenRouter -> OpenAI/custom -> Anthropic
pub async fn call_llm(
    prompt: &str,
    _task_hint: Option<&str>,
    max_tokens: Option<usize>,
) -> Result<String> {
    let providers = resolve_providers();
    if providers.is_empty() {
        anyhow::bail!("No LLM provider configured. Set OPENROUTER_API_KEY, OPENAI_API_KEY, or ANTHROPIC_API_KEY.");
    }

    for provider in &providers {
        match call_provider(provider, prompt, max_tokens).await {
            Ok(result) if !result.trim().is_empty() => return Ok(result),
            Ok(_) => {
                debug!(provider = provider.name, "Empty response, trying next");
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("402") || msg.contains("credit") || msg.contains("quota") {
                    debug!(provider = provider.name, "Credit exhausted, trying next");
                    continue;
                }
                warn!(provider = provider.name, error = %e, "Provider failed");
            }
        }
    }

    anyhow::bail!("All LLM providers failed")
}

/// Call an LLM with a specific model.
pub async fn call_llm_with_model(
    prompt: &str,
    model: &str,
    max_tokens: Option<usize>,
    temperature: Option<f64>,
) -> Result<String> {
    let providers = resolve_providers();
    if providers.is_empty() {
        anyhow::bail!("No LLM provider configured.");
    }

    let provider = &providers[0];
    let client = reqwest::Client::new();

    let mut body = json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
    });
    if let Some(mt) = max_tokens {
        body["max_tokens"] = json!(mt);
    }
    if let Some(temp) = temperature {
        body["temperature"] = json!(temp);
    }

    let resp = client
        .post(format!("{}/chat/completions", provider.base_url))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("HTTP request failed")?;

    let status = resp.status();
    let data: Value = resp.json().await.context("Failed to parse response")?;

    if !status.is_success() {
        anyhow::bail!("API error ({}): {}", status, data);
    }

    Ok(data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

struct ResolvedProvider {
    base_url: String,
    api_key: String,
    model: String,
    name: &'static str,
}

fn resolve_providers() -> Vec<ResolvedProvider> {
    let mut providers = Vec::new();

    // 1. OpenRouter
    if let Ok(key) = env::var("OPENROUTER_API_KEY") {
        if !key.is_empty() {
            providers.push(ResolvedProvider {
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: key,
                model: DEFAULT_AUX_MODEL.to_string(),
                name: "openrouter",
            });
        }
    }

    // 2. OpenAI / Custom
    if let Ok(key) = env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            let base = env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
            providers.push(ResolvedProvider {
                base_url: base,
                api_key: key,
                model: "gpt-4.1-mini".to_string(),
                name: "openai",
            });
        }
    }

    // 3. Anthropic (via OpenRouter-style endpoint)
    if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            providers.push(ResolvedProvider {
                base_url: "https://api.anthropic.com/v1".to_string(),
                api_key: key,
                model: "claude-sonnet-4".to_string(),
                name: "anthropic",
            });
        }
    }

    providers
}

async fn call_provider(
    provider: &ResolvedProvider,
    prompt: &str,
    max_tokens: Option<usize>,
) -> Result<String> {
    let client = reqwest::Client::new();

    // Special handling for Anthropic native API
    if provider.name == "anthropic" {
        return call_anthropic_native(provider, prompt, max_tokens).await;
    }

    let mut body = json!({
        "model": provider.model,
        "messages": [{"role": "user", "content": prompt}],
    });
    if let Some(mt) = max_tokens {
        body["max_tokens"] = json!(mt);
    }

    let resp = client
        .post(format!("{}/chat/completions", provider.base_url))
        .header("Authorization", format!("Bearer {}", provider.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("HTTP request failed")?;

    let status = resp.status();
    let data: Value = resp.json().await.context("Failed to parse response")?;

    if !status.is_success() {
        anyhow::bail!("API error ({}): {}", status, data);
    }

    Ok(data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

/// Call Anthropic's native Messages API.
async fn call_anthropic_native(
    provider: &ResolvedProvider,
    prompt: &str,
    max_tokens: Option<usize>,
) -> Result<String> {
    let client = reqwest::Client::new();
    let body = json!({
        "model": provider.model,
        "max_tokens": max_tokens.unwrap_or(4096),
        "messages": [{"role": "user", "content": prompt}],
    });

    let resp = client
        .post(format!("{}/messages", provider.base_url))
        .header("x-api-key", &provider.api_key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Anthropic request failed")?;

    let status = resp.status();
    let data: Value = resp.json().await.context("Failed to parse Anthropic response")?;

    if !status.is_success() {
        anyhow::bail!("Anthropic error ({}): {}", status, data);
    }

    // Extract text from content blocks
    let content = data["content"]
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| b["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_providers_empty() {
        // In test env without keys, should still not panic
        let _ = resolve_providers();
    }
}
