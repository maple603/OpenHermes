//! Anthropic Messages API adapter for OpenHermes Agent.
//!
//! Translates between OpenHermes's internal OpenAI-style message format and
//! Anthropic's Messages API. Handles auth (API key, OAuth Bearer, Claude Code
//! credentials), thinking budgets, and output limit lookups.

use std::collections::HashMap;
use std::env;

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use tracing::{debug, info};

/// Anthropic API base URL.
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Anthropic API version header.
pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Thinking budget configuration per effort level.
pub static THINKING_BUDGET: Lazy<HashMap<&'static str, usize>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("xhigh", 32000);
    m.insert("high", 16000);
    m.insert("medium", 8000);
    m.insert("low", 4000);
    m
});

/// Max output token limits per Anthropic model.
pub static ANTHROPIC_OUTPUT_LIMITS: Lazy<HashMap<&'static str, usize>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // Claude 4.6
    m.insert("claude-opus-4-6", 128_000);
    m.insert("claude-sonnet-4-6", 64_000);
    // Claude 4.5
    m.insert("claude-opus-4-5", 64_000);
    m.insert("claude-sonnet-4-5", 64_000);
    m.insert("claude-haiku-4-5", 64_000);
    // Claude 4
    m.insert("claude-opus-4", 32_000);
    m.insert("claude-sonnet-4", 16_384);
    // Claude 3.5
    m.insert("claude-3-5-sonnet", 8_192);
    m.insert("claude-3-5-haiku", 8_192);
    // Claude 3
    m.insert("claude-3-opus", 4_096);
    m.insert("claude-3-sonnet", 4_096);
    m.insert("claude-3-haiku", 4_096);
    m
});

/// Adaptive thinking effort map (OpenAI → Anthropic).
pub static ADAPTIVE_EFFORT_MAP: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("xhigh", "max");
    m.insert("high", "high");
    m.insert("medium", "medium");
    m.insert("low", "low");
    m.insert("minimal", "low");
    m
});

/// Authentication method resolved for the Anthropic API.
#[derive(Debug, Clone)]
pub enum AnthropicAuth {
    /// Standard API key (x-api-key header).
    ApiKey(String),
    /// OAuth Bearer token (Authorization: Bearer).
    Bearer(String),
}

/// Anthropic adapter for native Messages API calls.
pub struct AnthropicAdapter {
    auth: AnthropicAuth,
    client: reqwest::Client,
}

impl AnthropicAdapter {
    /// Create a new adapter, resolving auth from environment and credential files.
    pub fn new() -> Result<Self> {
        let auth = resolve_auth()?;
        Ok(Self {
            auth,
            client: reqwest::Client::new(),
        })
    }

    /// Get max output tokens for a model.
    pub fn max_output_tokens(model: &str) -> usize {
        let model_lower = model.to_lowercase();
        // Try exact match first
        for (key, limit) in ANTHROPIC_OUTPUT_LIMITS.iter() {
            if model_lower == *key {
                return *limit;
            }
        }
        // Try prefix match
        for (key, limit) in ANTHROPIC_OUTPUT_LIMITS.iter() {
            if model_lower.starts_with(key) {
                return *limit;
            }
        }
        // Default
        16_384
    }

    /// Call the Anthropic Messages API directly.
    pub async fn call(
        &self,
        messages: &[Value],
        model: &str,
        tools: Option<&[Value]>,
        max_tokens: Option<usize>,
        thinking_budget: Option<usize>,
    ) -> Result<Value> {
        let max_tokens = max_tokens.unwrap_or_else(|| Self::max_output_tokens(model));

        // Extract system message
        let (system_content, api_messages) = extract_system_message(messages);

        // Build request body
        let mut body = json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": api_messages,
        });

        if let Some(system) = system_content {
            body["system"] = json!(system);
        }

        if let Some(tools) = tools {
            if !tools.is_empty() {
                body["tools"] = json!(tools);
            }
        }

        if let Some(budget) = thinking_budget {
            body["thinking"] = json!({
                "type": "enabled",
                "budget_tokens": budget
            });
        }

        // Build headers
        let mut req = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("content-type", "application/json")
            .header("anthropic-version", ANTHROPIC_API_VERSION);

        match &self.auth {
            AnthropicAuth::ApiKey(key) => {
                req = req.header("x-api-key", key);
            }
            AnthropicAuth::Bearer(token) => {
                req = req.header("authorization", format!("Bearer {}", token));
                req = req.header("anthropic-beta", "oauth-2025-04-20");
            }
        }

        debug!(model = model, "Calling Anthropic Messages API");

        let response = req
            .json(&body)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        let status = response.status();
        let response_body: Value = response
            .json()
            .await
            .context("Failed to parse Anthropic API response")?;

        if !status.is_success() {
            let error_msg = response_body
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error");
            anyhow::bail!(
                "Anthropic API error ({}): {}",
                status.as_u16(),
                error_msg
            );
        }

        Ok(response_body)
    }
}

/// Convert OpenAI-format messages to Anthropic format.
///
/// - Merges consecutive same-role messages
/// - Converts tool_calls to tool_use blocks
/// - Converts tool results to tool_result blocks
pub fn convert_openai_to_anthropic(messages: &[Value]) -> Vec<Value> {
    let mut result = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

        match role {
            "system" => {
                // System messages are handled separately
                continue;
            }
            "user" => {
                let content = msg.get("content").cloned().unwrap_or(Value::Null);
                merge_or_push(&mut result, "user", content);
            }
            "assistant" => {
                let mut blocks = Vec::new();

                // Text content
                if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                    if !content.is_empty() {
                        blocks.push(json!({"type": "text", "text": content}));
                    }
                }

                // Tool calls → tool_use blocks
                if let Some(tool_calls) = msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                    for tc in tool_calls {
                        let id = tc
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("unknown");
                        let func = tc.get("function").unwrap_or(&Value::Null);
                        let name = func
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown");
                        let args_str = func
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        let input: Value =
                            serde_json::from_str(args_str).unwrap_or(json!({}));

                        blocks.push(json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input
                        }));
                    }
                }

                if !blocks.is_empty() {
                    merge_or_push(&mut result, "assistant", json!(blocks));
                }
            }
            "tool" => {
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let tool_call_id = msg
                    .get("tool_call_id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("unknown");

                let block = json!([{
                    "type": "tool_result",
                    "tool_use_id": tool_call_id,
                    "content": content
                }]);

                merge_or_push(&mut result, "user", block);
            }
            _ => {}
        }
    }

    result
}

/// Convert Anthropic response to OpenAI-format response.
pub fn convert_anthropic_to_openai(response: &Value) -> Value {
    let mut content = String::new();
    let mut tool_calls = Vec::new();

    if let Some(content_blocks) = response.get("content").and_then(|c| c.as_array()) {
        for block in content_blocks {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        if !content.is_empty() {
                            content.push('\n');
                        }
                        content.push_str(text);
                    }
                }
                "tool_use" => {
                    let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let empty_obj = json!({});
                    let input = block.get("input").unwrap_or(&empty_obj);
                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": serde_json::to_string(input).unwrap_or_default()
                        }
                    }));
                }
                "thinking" => {
                    // Skip thinking blocks in the conversion
                    debug!("Skipping thinking block in conversion");
                }
                _ => {}
            }
        }
    }

    let mut choice = json!({
        "message": {
            "role": "assistant",
            "content": if content.is_empty() { Value::Null } else { json!(content) }
        },
        "finish_reason": response.get("stop_reason").and_then(|r| r.as_str()).unwrap_or("stop")
    });

    if !tool_calls.is_empty() {
        choice["message"]["tool_calls"] = json!(tool_calls);
        choice["finish_reason"] = json!("tool_calls");
    }

    json!({
        "choices": [choice],
        "model": response.get("model").cloned().unwrap_or(Value::Null),
        "usage": response.get("usage").cloned().unwrap_or(Value::Null)
    })
}

// ── Auth resolution ─────────────────────────────────────────────────────

fn resolve_auth() -> Result<AnthropicAuth> {
    // 1. ANTHROPIC_API_KEY env var
    if let Ok(key) = env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            if key.starts_with("sk-ant-oat") {
                // OAuth setup token
                return Ok(AnthropicAuth::Bearer(key));
            }
            return Ok(AnthropicAuth::ApiKey(key));
        }
    }

    // 2. Claude Code credentials (~/.claude.json or ~/.claude/.credentials.json)
    if let Some(token) = read_claude_credentials() {
        info!("Using Claude Code credentials for Anthropic auth");
        return Ok(AnthropicAuth::Bearer(token));
    }

    anyhow::bail!(
        "No Anthropic API key found. Set ANTHROPIC_API_KEY or install Claude Code."
    )
}

fn read_claude_credentials() -> Option<String> {
    let home = dirs::home_dir()?;

    // Try ~/.claude.json
    let claude_json = home.join(".claude.json");
    if let Ok(data) = std::fs::read_to_string(&claude_json) {
        if let Ok(parsed) = serde_json::from_str::<Value>(&data) {
            if let Some(token) = parsed
                .get("oauthToken")
                .and_then(|t| t.as_str())
                .filter(|t| !t.is_empty())
            {
                return Some(token.to_string());
            }
        }
    }

    // Try ~/.claude/.credentials.json
    let creds_json = home.join(".claude").join(".credentials.json");
    if let Ok(data) = std::fs::read_to_string(&creds_json) {
        if let Ok(parsed) = serde_json::from_str::<Value>(&data) {
            if let Some(token) = parsed
                .get("token")
                .and_then(|t| t.as_str())
                .filter(|t| !t.is_empty())
            {
                return Some(token.to_string());
            }
        }
    }

    None
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Extract system message from the message list.
fn extract_system_message(messages: &[Value]) -> (Option<String>, Vec<Value>) {
    let mut system = None;
    let mut rest = Vec::new();

    for msg in messages {
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role == "system" {
            if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                system = Some(content.to_string());
            }
        } else {
            rest.push(msg.clone());
        }
    }

    (system, rest)
}

/// Merge content into the last message if same role, otherwise push new message.
fn merge_or_push(messages: &mut Vec<Value>, role: &str, content: Value) {
    if let Some(last) = messages.last_mut() {
        if last.get("role").and_then(|r| r.as_str()) == Some(role) {
            // Merge: append content blocks
            if let Some(existing) = last.get_mut("content") {
                if let (Some(existing_arr), Some(new_arr)) =
                    (existing.as_array_mut(), content.as_array())
                {
                    existing_arr.extend(new_arr.iter().cloned());
                    return;
                }
            }
        }
    }

    messages.push(json!({"role": role, "content": content}));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_max_output_tokens() {
        assert_eq!(AnthropicAdapter::max_output_tokens("claude-opus-4-6"), 128_000);
        assert_eq!(AnthropicAdapter::max_output_tokens("claude-sonnet-4"), 16_384);
        assert_eq!(AnthropicAdapter::max_output_tokens("claude-3-5-sonnet-20241022"), 8_192);
        assert_eq!(AnthropicAdapter::max_output_tokens("unknown-model"), 16_384);
    }

    #[test]
    fn test_convert_openai_to_anthropic_basic() {
        let messages = vec![
            json!({"role": "system", "content": "You are helpful."}),
            json!({"role": "user", "content": "Hello"}),
            json!({"role": "assistant", "content": "Hi!"}),
            json!({"role": "user", "content": "How are you?"}),
        ];

        let result = convert_openai_to_anthropic(&messages);
        // System message should be excluded
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
        assert_eq!(result[2]["role"], "user");
    }

    #[test]
    fn test_convert_openai_to_anthropic_tool_calls() {
        let messages = vec![
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {
                        "name": "web_search",
                        "arguments": "{\"query\": \"rust\"}"
                    }
                }]
            }),
            json!({
                "role": "tool",
                "content": "Search results...",
                "tool_call_id": "call_1"
            }),
        ];

        let result = convert_openai_to_anthropic(&messages);
        assert_eq!(result.len(), 2);

        // Assistant should have tool_use block
        let blocks = result[0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "tool_use");
        assert_eq!(blocks[0]["name"], "web_search");

        // Tool result should be user message
        assert_eq!(result[1]["role"], "user");
    }

    #[test]
    fn test_convert_anthropic_to_openai() {
        let response = json!({
            "content": [
                {"type": "text", "text": "Hello!"},
                {"type": "tool_use", "id": "call_1", "name": "read_file", "input": {"path": "test.txt"}}
            ],
            "model": "claude-sonnet-4",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 100, "output_tokens": 50}
        });

        let result = convert_anthropic_to_openai(&response);
        let choice = &result["choices"][0];
        assert_eq!(choice["message"]["content"], "Hello!");
        assert_eq!(choice["finish_reason"], "tool_calls");

        let tool_calls = choice["message"]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["function"]["name"], "read_file");
    }

    #[test]
    fn test_extract_system_message() {
        let messages = vec![
            json!({"role": "system", "content": "You are helpful."}),
            json!({"role": "user", "content": "Hello"}),
        ];

        let (system, rest) = extract_system_message(&messages);
        assert_eq!(system, Some("You are helpful.".to_string()));
        assert_eq!(rest.len(), 1);
    }

    #[test]
    fn test_thinking_budget() {
        assert_eq!(THINKING_BUDGET.get("xhigh"), Some(&32000));
        assert_eq!(THINKING_BUDGET.get("high"), Some(&16000));
        assert_eq!(THINKING_BUDGET.get("medium"), Some(&8000));
        assert_eq!(THINKING_BUDGET.get("low"), Some(&4000));
    }
}
