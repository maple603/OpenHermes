//! Anthropic prompt caching (system_and_3 strategy).
//!
//! Reduces input token costs by ~75% on multi-turn conversations by caching
//! the conversation prefix. Uses 4 cache_control breakpoints (Anthropic max):
//!   1. System prompt (stable across all turns)
//!   2-4. Last 3 non-system messages (rolling window)
//!
//! Pure functions — no class state, no AIAgent dependency.

use serde_json::{json, Value};

/// Cache marker for Anthropic's prompt caching.
#[allow(dead_code)]
const CACHE_MARKER: Value = Value::String(String::new());

fn make_cache_marker() -> Value {
    json!({"type": "ephemeral"})
}

/// Apply cache_control to a single message value, handling all format variations.
///
/// - String content → wrap in `[{type: "text", text: ..., cache_control: marker}]`
/// - Array content → add cache_control to last element
/// - Tool message with native_anthropic → add cache_control at message level
fn apply_cache_marker(msg: &mut Value, native_anthropic: bool) {
    let marker = make_cache_marker();
    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");

    if role == "tool" {
        if native_anthropic {
            msg["cache_control"] = marker;
        }
        return;
    }

    let content = msg.get("content").cloned();

    match content {
        None | Some(Value::Null) => {
            msg["cache_control"] = marker;
        }
        Some(Value::String(ref s)) if s.is_empty() => {
            msg["cache_control"] = marker;
        }
        Some(Value::String(s)) => {
            msg["content"] = json!([{
                "type": "text",
                "text": s,
                "cache_control": marker
            }]);
        }
        Some(Value::Array(ref arr)) if !arr.is_empty() => {
            if let Some(content_arr) = msg.get_mut("content").and_then(|c| c.as_array_mut()) {
                if let Some(last) = content_arr.last_mut() {
                    if let Some(obj) = last.as_object_mut() {
                        obj.insert("cache_control".to_string(), marker);
                    }
                }
            }
        }
        _ => {
            msg["cache_control"] = marker;
        }
    }
}

/// Apply system_and_3 caching strategy to messages for Anthropic models.
///
/// Places up to 4 cache_control breakpoints:
/// - System prompt (index 0)
/// - Last 3 non-system messages (rolling window)
///
/// Returns the annotated messages (cloned).
pub fn apply_anthropic_cache_control(
    api_messages: &[Value],
    native_anthropic: bool,
) -> Vec<Value> {
    if api_messages.is_empty() {
        return Vec::new();
    }

    let mut messages: Vec<Value> = api_messages.to_vec();

    // Find system message index (usually 0)
    let system_idx = messages
        .iter()
        .position(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"));

    // Mark system prompt
    if let Some(idx) = system_idx {
        apply_cache_marker(&mut messages[idx], native_anthropic);
    }

    // Find last 3 non-system messages
    let non_system_indices: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| m.get("role").and_then(|r| r.as_str()) != Some("system"))
        .map(|(i, _)| i)
        .collect();

    // Mark last 3 non-system messages (up to 3 breakpoints remaining)
    let mark_count = non_system_indices.len().min(3);
    if mark_count > 0 {
        let start = non_system_indices.len() - mark_count;
        for &idx in &non_system_indices[start..] {
            apply_cache_marker(&mut messages[idx], native_anthropic);
        }
    }

    messages
}

/// Check if a model name suggests Anthropic prompt caching should be used.
pub fn should_use_prompt_caching(model: &str, base_url: Option<&str>) -> bool {
    let model_lower = model.to_lowercase();
    let is_claude = model_lower.contains("claude");

    // Only use prompt caching with Anthropic models
    if !is_claude {
        return false;
    }

    // If using OpenRouter or another proxy, caching still works
    // as long as the model is Claude
    if let Some(url) = base_url {
        let url_lower = url.to_lowercase();
        // Prompt caching works via OpenRouter and native Anthropic
        return url_lower.contains("anthropic.com") || url_lower.contains("openrouter.ai");
    }

    // Default: enable for Claude models
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_apply_cache_control_basic() {
        let messages = vec![
            json!({"role": "system", "content": "You are helpful."}),
            json!({"role": "user", "content": "Hello"}),
            json!({"role": "assistant", "content": "Hi there!"}),
            json!({"role": "user", "content": "How are you?"}),
        ];

        let result = apply_anthropic_cache_control(&messages, false);

        // System message should have cache_control
        assert!(result[0]["content"][0]["cache_control"].is_object());

        // Last 3 non-system messages should have cache_control
        assert!(result[1]["content"][0]["cache_control"].is_object());
        assert!(result[2]["content"][0]["cache_control"].is_object());
        assert!(result[3]["content"][0]["cache_control"].is_object());
    }

    #[test]
    fn test_apply_cache_control_many_messages() {
        let messages = vec![
            json!({"role": "system", "content": "System prompt"}),
            json!({"role": "user", "content": "msg1"}),
            json!({"role": "assistant", "content": "resp1"}),
            json!({"role": "user", "content": "msg2"}),
            json!({"role": "assistant", "content": "resp2"}),
            json!({"role": "user", "content": "msg3"}),
        ];

        let result = apply_anthropic_cache_control(&messages, false);

        // System always cached
        assert!(result[0]["content"][0]["cache_control"].is_object());

        // Only last 3 non-system get cache markers (indices 3, 4, 5)
        // Earlier non-system (1, 2) should NOT have cache_control
        assert!(!result[1]["content"][0].get("cache_control").is_some()
            || result[1]["content"].is_string()); // string content means no cache marker applied
        assert!(result[4]["content"][0]["cache_control"].is_object());
        assert!(result[5]["content"][0]["cache_control"].is_object());
    }

    #[test]
    fn test_apply_cache_control_empty() {
        let result = apply_anthropic_cache_control(&[], false);
        assert!(result.is_empty());
    }

    #[test]
    fn test_should_use_prompt_caching() {
        assert!(should_use_prompt_caching("claude-sonnet-4", Some("https://api.anthropic.com/v1")));
        assert!(should_use_prompt_caching("claude-opus-4-6", Some("https://openrouter.ai/api/v1")));
        assert!(!should_use_prompt_caching("gpt-4o", Some("https://api.openai.com/v1")));
        assert!(!should_use_prompt_caching("gemini-2.5-pro", None));
    }

    #[test]
    fn test_native_anthropic_tool_caching() {
        let messages = vec![
            json!({"role": "tool", "content": "tool result", "tool_call_id": "call_1"}),
        ];

        // With native_anthropic=true, tool messages get cache_control at message level
        let result = apply_anthropic_cache_control(&messages, true);
        assert!(result[0]["cache_control"].is_object());

        // With native_anthropic=false, tool messages should NOT get cache_control
        let result2 = apply_anthropic_cache_control(&messages, false);
        assert!(result2[0].get("cache_control").is_none());
    }
}
