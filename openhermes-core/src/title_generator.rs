//! Auto-generate short session titles from the first user/assistant exchange.
//!
//! Runs asynchronously after the first response is delivered so it never
//! adds latency to the user-facing reply.

use async_openai::config::OpenAIConfig;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessage,
    ChatCompletionRequestUserMessageContent, CreateChatCompletionRequest,
};
use async_openai::Client;
use tracing::{debug, warn};

/// System prompt used to generate concise session titles.
const TITLE_PROMPT: &str = "\
Generate a short, descriptive title (3-7 words) for a conversation that starts with the \
following exchange. The title should capture the main topic or intent. \
Return ONLY the title text, nothing else. No quotes, no punctuation at the end, no prefixes.";

/// Maximum character length for a generated title.
const MAX_TITLE_LEN: usize = 80;

// ---------------------------------------------------------------------------
// Core generation
// ---------------------------------------------------------------------------

/// Generate a session title from the first user/assistant exchange.
///
/// Uses the provided OpenAI-compatible client with a small `max_tokens` and
/// low temperature for deterministic, cheap results.
///
/// Returns `None` on any failure (network, parse, empty).
pub async fn generate_title(
    client: &Client<OpenAIConfig>,
    model: &str,
    user_msg: &str,
    assistant_msg: &str,
) -> Option<String> {
    // Truncate long messages to keep the request small.
    let user_snippet = truncate(user_msg, 500);
    let assistant_snippet = truncate(assistant_msg, 500);

    let messages = vec![
        ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: ChatCompletionRequestSystemMessageContent::Text(TITLE_PROMPT.to_string()),
            name: None,
        }),
        ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: ChatCompletionRequestUserMessageContent::Text(format!(
                "User: {}\n\nAssistant: {}",
                user_snippet, assistant_snippet
            )),
            name: None,
        }),
    ];

    let request = CreateChatCompletionRequest {
        model: model.to_string(),
        messages,
        max_completion_tokens: Some(30),
        temperature: Some(0.3),
        ..Default::default()
    };

    let response = match client.chat().create(request).await {
        Ok(r) => r,
        Err(e) => {
            warn!("Title generation failed: {}", e);
            return None;
        }
    };

    let raw_title = response
        .choices
        .first()?
        .message
        .content
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();

    Some(clean_title(&raw_title))
}

// ---------------------------------------------------------------------------
// Fire-and-forget helper
// ---------------------------------------------------------------------------

/// Spawn a background task to generate and deliver a title.
///
/// `on_title` is called with the generated title if successful.
pub fn spawn_title_generation<F>(
    client: Client<OpenAIConfig>,
    model: String,
    user_msg: String,
    assistant_msg: String,
    on_title: F,
) where
    F: FnOnce(String) + Send + 'static,
{
    tokio::spawn(async move {
        if let Some(title) = generate_title(&client, &model, &user_msg, &assistant_msg).await {
            if !title.is_empty() {
                debug!(title = %title, "Session title generated");
                on_title(title);
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Clean up a raw title: strip quotes, remove common prefixes, enforce max length.
fn clean_title(raw: &str) -> String {
    let mut title = raw.trim().to_string();

    // Strip surrounding quotes
    if (title.starts_with('"') && title.ends_with('"'))
        || (title.starts_with('\'') && title.ends_with('\''))
    {
        title = title[1..title.len() - 1].to_string();
    }

    // Remove common prefixes
    let prefixes = ["Title:", "title:", "TITLE:"];
    for prefix in &prefixes {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }

    // Strip trailing punctuation
    while title.ends_with('.') || title.ends_with('!') || title.ends_with('?') {
        title.pop();
    }

    // Enforce maximum length
    if title.len() > MAX_TITLE_LEN {
        title = title.chars().take(MAX_TITLE_LEN).collect();
        // Try to break at a word boundary
        if let Some(pos) = title.rfind(' ') {
            title.truncate(pos);
        }
    }

    title.trim().to_string()
}

/// Truncate a string to at most `max_chars` characters.
fn truncate(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_title_strips_quotes() {
        assert_eq!(clean_title("\"Setup Rust Project\""), "Setup Rust Project");
        assert_eq!(clean_title("'Fix Bug in Parser'"), "Fix Bug in Parser");
    }

    #[test]
    fn test_clean_title_removes_prefix() {
        assert_eq!(clean_title("Title: Hello World"), "Hello World");
    }

    #[test]
    fn test_clean_title_strips_punctuation() {
        assert_eq!(clean_title("Debug API Error."), "Debug API Error");
    }

    #[test]
    fn test_clean_title_max_length() {
        let long = "A ".repeat(60);
        let cleaned = clean_title(&long);
        assert!(cleaned.len() <= MAX_TITLE_LEN);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("hi", 10), "hi");
    }
}
