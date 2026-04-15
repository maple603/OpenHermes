//! Model metadata, context lengths, and token estimation utilities.
//!
//! Pure utility functions with no AIAgent dependency. Used by ContextCompressor,
//! prompt caching, and context references for pre-flight context checks.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;

// ── Provider prefixes that can appear before a model ID ─────────────────

static PROVIDER_PREFIXES: Lazy<Vec<&'static str>> = Lazy::new(|| vec![
    "openrouter", "nous", "openai-codex", "copilot", "copilot-acp",
    "gemini", "zai", "kimi-coding", "kimi-coding-cn", "minimax", "minimax-cn",
    "anthropic", "deepseek", "opencode-zen", "opencode-go", "ai-gateway",
    "kilocode", "alibaba", "qwen-oauth", "xiaomi", "arcee",
    "custom", "local",
    // Common aliases
    "google", "google-gemini", "google-ai-studio",
    "glm", "z-ai", "z.ai", "zhipu", "github", "github-copilot",
    "github-models", "kimi", "moonshot", "kimi-cn", "moonshot-cn",
    "claude", "deep-seek", "opencode", "zen", "go", "vercel", "kilo",
    "dashscope", "aliyun", "qwen", "mimo", "xiaomi-mimo",
    "arcee-ai", "arceeai", "qwen-portal",
]);

/// Regex to detect Ollama-style model:tag suffixes (e.g. "7b", "latest", "q4_0").
static OLLAMA_TAG_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\d+\.?\d*b|latest|stable|q\d|fp?\d|instruct|chat|coder|vision|text)")
        .expect("valid regex")
});

/// Default context lengths for known model families.
/// Keys are matched as prefixes (longest match wins).
static DEFAULT_CONTEXT_LENGTHS: Lazy<Vec<(&'static str, usize)>> = Lazy::new(|| {
    // Sorted by key length descending so longest prefix matches first.
    let mut entries = vec![
        // Anthropic Claude 4.6 (1M context)
        ("claude-opus-4-6", 1_000_000),
        ("claude-sonnet-4-6", 1_000_000),
        ("claude-opus-4.6", 1_000_000),
        ("claude-sonnet-4.6", 1_000_000),
        // Claude catch-all
        ("claude", 200_000),
        // OpenAI — GPT-5 family
        ("gpt-5.4-nano", 400_000),
        ("gpt-5.4-mini", 400_000),
        ("gpt-5.4", 1_050_000),
        ("gpt-5.3-codex-spark", 128_000),
        ("gpt-5.1-chat", 128_000),
        ("gpt-5", 400_000),
        ("gpt-4.1", 1_047_576),
        ("gpt-4", 128_000),
        // Google
        ("gemini", 1_048_576),
        // Gemma
        ("gemma-4-31b", 256_000),
        ("gemma-4-26b", 256_000),
        ("gemma-3", 131_072),
        ("gemma", 8_192),
        // DeepSeek
        ("deepseek", 128_000),
        // Meta
        ("llama", 131_072),
        // Qwen
        ("qwen3-coder-plus", 1_000_000),
        ("qwen3-coder", 262_144),
        ("qwen", 131_072),
        // MiniMax
        ("minimax", 204_800),
        // GLM
        ("glm", 202_752),
        // xAI Grok
        ("grok-code-fast", 256_000),
        ("grok-4-1-fast", 2_000_000),
        ("grok-2-vision", 8_192),
        ("grok-4-fast", 2_000_000),
        ("grok-4.20", 2_000_000),
        ("grok-4", 256_000),
        ("grok-3", 131_072),
        ("grok-2", 131_072),
        ("grok", 131_072),
        // Kimi
        ("kimi", 262_144),
        // Arcee
        ("trinity", 262_144),
        // Hugging Face Inference Providers
        ("Qwen/Qwen3.5-397B-A17B", 131_072),
        ("Qwen/Qwen3.5-35B-A3B", 131_072),
        ("deepseek-ai/DeepSeek-V3.2", 65_536),
        ("moonshotai/Kimi-K2.5", 262_144),
        ("moonshotai/Kimi-K2-Thinking", 262_144),
        ("MiniMaxAI/MiniMax-M2.5", 204_800),
        ("XiaomiMiMo/MiMo-V2-Flash", 256_000),
        ("mimo-v2-pro", 1_000_000),
        ("mimo-v2-omni", 256_000),
        ("mimo-v2-flash", 256_000),
        ("zai-org/GLM-5", 202_752),
        // Mistral
        ("mistral", 131_072),
        // Cohere
        ("command-r", 131_072),
        ("command", 131_072),
        // Yi
        ("yi", 200_000),
    ];
    // Sort by key length descending for longest-prefix-match
    entries.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    entries
});

/// URL-to-provider mapping for inferring provider from base URL.
static URL_TO_PROVIDER: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("api.openai.com", "openai");
    m.insert("api.anthropic.com", "anthropic");
    m.insert("api.z.ai", "zai");
    m.insert("api.moonshot.ai", "kimi");
    m.insert("api.arcee.ai", "arcee");
    m.insert("openrouter.ai", "openrouter");
    m.insert("generativelanguage.googleapis.com", "gemini");
    m.insert("inference-api.nousresearch.com", "nous");
    m.insert("api.deepseek.com", "deepseek");
    m.insert("api.x.ai", "xai");
    m.insert("dashscope.aliyuncs.com", "alibaba");
    m
});

/// Default context length when no detection method succeeds.
pub const DEFAULT_FALLBACK_CONTEXT: usize = 128_000;

/// Minimum context length required to run Hermes Agent.
pub const MINIMUM_CONTEXT_LENGTH: usize = 64_000;

/// Context probe tiers for unknown models.
pub const CONTEXT_PROBE_TIERS: &[usize] = &[128_000, 64_000, 32_000, 16_000, 8_000];

// ── Public API ──────────────────────────────────────────────────────────

/// Strip a recognized provider prefix from a model string.
///
/// `"local:my-model"` → `"my-model"`
/// `"qwen3.5:27b"` → `"qwen3.5:27b"` (unchanged — Ollama model:tag)
pub fn strip_provider_prefix(model: &str) -> &str {
    if !model.contains(':') || model.starts_with("http") {
        return model;
    }

    if let Some((prefix, suffix)) = model.split_once(':') {
        let prefix_lower = prefix.trim().to_lowercase();
        if PROVIDER_PREFIXES.iter().any(|p| *p == prefix_lower) {
            let suffix_trimmed = suffix.trim();
            // Don't strip if suffix looks like an Ollama tag
            if OLLAMA_TAG_PATTERN.is_match(suffix_trimmed) {
                return model;
            }
            return suffix_trimmed;
        }
    }

    model
}

/// Get the context length for a model, using a fallback chain.
///
/// Resolution order:
/// 1. Exact match in DEFAULT_CONTEXT_LENGTHS
/// 2. Prefix match (longest prefix wins)
/// 3. Provider inference from base_url
/// 4. DEFAULT_FALLBACK_CONTEXT (128K)
pub fn get_model_context_length(model: &str, base_url: Option<&str>) -> usize {
    let stripped = strip_provider_prefix(model);
    let lower = stripped.to_lowercase();

    // 1. Exact match
    for (key, ctx) in DEFAULT_CONTEXT_LENGTHS.iter() {
        if lower == key.to_lowercase() {
            return *ctx;
        }
    }

    // 2. Prefix match (already sorted longest-first)
    for (key, ctx) in DEFAULT_CONTEXT_LENGTHS.iter() {
        if lower.starts_with(&key.to_lowercase()) {
            return *ctx;
        }
    }

    // 3. Provider inference from base_url
    if let Some(url) = base_url {
        let url_lower = url.to_lowercase();
        for (domain, _provider) in URL_TO_PROVIDER.iter() {
            if url_lower.contains(domain) {
                // Known provider — use the model prefix match on provider-specific models
                // For now, return the default for that provider family
                if url_lower.contains("anthropic") {
                    return 200_000;
                }
                if url_lower.contains("openai") {
                    return 128_000;
                }
                if url_lower.contains("generativelanguage") {
                    return 1_048_576;
                }
            }
        }
    }

    // 4. Fallback
    DEFAULT_FALLBACK_CONTEXT
}

/// Rough token estimation: ~4 characters per token for English text.
pub fn estimate_tokens_rough(text: &str) -> usize {
    text.len() / 4
}

/// Estimate tokens for a serialized message content string.
/// Adds per-message overhead (~4 tokens for role/formatting).
pub fn estimate_message_tokens(content: &str) -> usize {
    estimate_tokens_rough(content) + 4
}

/// Check if a model name indicates an Anthropic model.
pub fn is_anthropic_model(model: &str) -> bool {
    let stripped = strip_provider_prefix(model).to_lowercase();
    stripped.starts_with("claude")
}

/// Check if a base URL points to the Anthropic API.
pub fn is_anthropic_url(base_url: &str) -> bool {
    base_url.to_lowercase().contains("anthropic.com")
}

/// Infer the provider name from a base URL.
pub fn infer_provider_from_url(base_url: &str) -> Option<&'static str> {
    let lower = base_url.to_lowercase();
    for (domain, provider) in URL_TO_PROVIDER.iter() {
        if lower.contains(domain) {
            return Some(provider);
        }
    }
    None
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_provider_prefix() {
        assert_eq!(strip_provider_prefix("anthropic:claude-sonnet-4"), "claude-sonnet-4");
        assert_eq!(strip_provider_prefix("openrouter:meta-llama/llama-3"), "meta-llama/llama-3");
        assert_eq!(strip_provider_prefix("local:my-model"), "my-model");
        // Ollama tags should NOT be stripped
        assert_eq!(strip_provider_prefix("qwen3.5:27b"), "qwen3.5:27b");
        assert_eq!(strip_provider_prefix("deepseek:latest"), "deepseek:latest");
        // No colon → unchanged
        assert_eq!(strip_provider_prefix("gpt-4o"), "gpt-4o");
        // HTTP URL → unchanged
        assert_eq!(strip_provider_prefix("http://localhost:8080"), "http://localhost:8080");
    }

    #[test]
    fn test_context_length_exact() {
        assert_eq!(get_model_context_length("claude-opus-4-6", None), 1_000_000);
        assert_eq!(get_model_context_length("gpt-5.4", None), 1_050_000);
        assert_eq!(get_model_context_length("gemini", None), 1_048_576);
    }

    #[test]
    fn test_context_length_prefix() {
        assert_eq!(get_model_context_length("claude-sonnet-4", None), 200_000);
        assert_eq!(get_model_context_length("gpt-5-turbo", None), 400_000);
        assert_eq!(get_model_context_length("deepseek-v3", None), 128_000);
        assert_eq!(get_model_context_length("llama-3.1-405b", None), 131_072);
    }

    #[test]
    fn test_context_length_with_provider_prefix() {
        // Provider prefix should be stripped before lookup
        assert_eq!(get_model_context_length("anthropic:claude-opus-4-6", None), 1_000_000);
        assert_eq!(get_model_context_length("openrouter:gpt-5.4", None), 1_050_000);
    }

    #[test]
    fn test_context_length_fallback() {
        assert_eq!(get_model_context_length("unknown-model-xyz", None), DEFAULT_FALLBACK_CONTEXT);
    }

    #[test]
    fn test_estimate_tokens() {
        // 100 chars → ~25 tokens
        let text = "a".repeat(100);
        assert_eq!(estimate_tokens_rough(&text), 25);

        // Empty
        assert_eq!(estimate_tokens_rough(""), 0);
    }

    #[test]
    fn test_is_anthropic_model() {
        assert!(is_anthropic_model("claude-sonnet-4"));
        assert!(is_anthropic_model("anthropic:claude-opus-4-6"));
        assert!(!is_anthropic_model("gpt-4o"));
        assert!(!is_anthropic_model("gemini-2.5-pro"));
    }

    #[test]
    fn test_infer_provider() {
        assert_eq!(infer_provider_from_url("https://api.openai.com/v1"), Some("openai"));
        assert_eq!(infer_provider_from_url("https://api.anthropic.com/v1/messages"), Some("anthropic"));
        assert_eq!(infer_provider_from_url("https://openrouter.ai/api/v1"), Some("openrouter"));
        assert_eq!(infer_provider_from_url("http://localhost:8080"), None);
    }
}
