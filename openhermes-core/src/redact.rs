//! Regex-based secret redaction for logs and tool output.
//!
//! Applies pattern matching to mask API keys, tokens, and credentials
//! before they reach log files, verbose output, or gateway logs.

use once_cell::sync::Lazy;
use regex::Regex;

/// Whether redaction is enabled (checked at startup from HERMES_REDACT_SECRETS env).
static REDACT_ENABLED: Lazy<bool> = Lazy::new(|| {
    match std::env::var("HERMES_REDACT_SECRETS") {
        Ok(v) => !matches!(v.to_lowercase().as_str(), "0" | "false" | "no" | "off"),
        Err(_) => true, // enabled by default
    }
});

// ---------------------------------------------------------------------------
// Known API key prefix patterns
// ---------------------------------------------------------------------------

static PREFIX_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    let patterns = [
        r"sk-[A-Za-z0-9_-]{10,}",           // OpenAI / OpenRouter / Anthropic
        r"ghp_[A-Za-z0-9]{10,}",            // GitHub PAT (classic)
        r"github_pat_[A-Za-z0-9_]{10,}",    // GitHub PAT (fine-grained)
        r"gho_[A-Za-z0-9]{10,}",            // GitHub OAuth access token
        r"ghu_[A-Za-z0-9]{10,}",            // GitHub user-to-server token
        r"ghs_[A-Za-z0-9]{10,}",            // GitHub server-to-server token
        r"ghr_[A-Za-z0-9]{10,}",            // GitHub refresh token
        r"xox[baprs]-[A-Za-z0-9-]{10,}",    // Slack tokens
        r"AIza[A-Za-z0-9_-]{30,}",          // Google API keys
        r"pplx-[A-Za-z0-9]{10,}",           // Perplexity
        r"fal_[A-Za-z0-9_-]{10,}",          // Fal.ai
        r"fc-[A-Za-z0-9]{10,}",             // Firecrawl
        r"bb_live_[A-Za-z0-9_-]{10,}",      // BrowserBase
        r"gAAAA[A-Za-z0-9_=-]{20,}",        // Codex encrypted tokens
        r"AKIA[A-Z0-9]{16}",                // AWS Access Key ID
        r"sk_live_[A-Za-z0-9]{10,}",        // Stripe secret key (live)
        r"sk_test_[A-Za-z0-9]{10,}",        // Stripe secret key (test)
        r"rk_live_[A-Za-z0-9]{10,}",        // Stripe restricted key
        r"SG\.[A-Za-z0-9_-]{10,}",          // SendGrid API key
        r"hf_[A-Za-z0-9]{10,}",             // HuggingFace token
        r"r8_[A-Za-z0-9]{10,}",             // Replicate API token
        r"npm_[A-Za-z0-9]{10,}",            // npm access token
        r"pypi-[A-Za-z0-9_-]{10,}",         // PyPI API token
        r"dop_v1_[A-Za-z0-9]{10,}",         // DigitalOcean PAT
        r"doo_v1_[A-Za-z0-9]{10,}",         // DigitalOcean OAuth
        r"am_[A-Za-z0-9_-]{10,}",           // AgentMail API key
        r"sk_[A-Za-z0-9_]{10,}",            // ElevenLabs TTS key
        r"tvly-[A-Za-z0-9]{10,}",           // Tavily search API key
        r"exa_[A-Za-z0-9]{10,}",            // Exa search API key
    ];
    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
});

// ---------------------------------------------------------------------------
// Contextual patterns
// ---------------------------------------------------------------------------

static CONTEXTUAL_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    let patterns = [
        // ENV variable assignments: FOO_KEY=somevalue or export BAR_SECRET=val
        r#"(?i)([\w]*(?:KEY|TOKEN|SECRET|PASSWORD|CREDENTIAL|AUTH)[_\w]*)=([^\s"']{8,})"#,
        // JSON fields with sensitive names
        r#"(?i)"(?:password|secret|token|api_key|apiKey|access_token|refresh_token|private_key|authorization)"\s*:\s*"([^"]{8,})""#,
        // Authorization headers
        r"(?i)(?:Authorization|Bearer|Basic)\s+([A-Za-z0-9_/+=-]{20,})",
        // Private key blocks
        r"-----BEGIN (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----",
        // Database connection strings with passwords
        r"(?i)(?:postgres|mysql|mongodb|redis)://[^:]+:([^@]{3,})@",
        // Telegram bot tokens
        r"\d{8,10}:[A-Za-z0-9_-]{35}",
    ];
    patterns
        .iter()
        .filter_map(|p| Regex::new(p).ok())
        .collect()
});

// ---------------------------------------------------------------------------
// Masking
// ---------------------------------------------------------------------------

/// Mask a token: short tokens become `***`, longer ones keep prefix + suffix.
fn mask_token(token: &str) -> String {
    if token.len() < 18 {
        "***".to_string()
    } else {
        let prefix: String = token.chars().take(6).collect();
        let suffix: String = token.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
        format!("{}...{}", prefix, suffix)
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Redact sensitive tokens and credentials from text.
///
/// Returns the text with all recognised secrets replaced by masked versions.
/// If `HERMES_REDACT_SECRETS` env is set to `false`/`0`/`no`/`off`, returns text unchanged.
pub fn redact_sensitive_text(text: &str) -> String {
    if !*REDACT_ENABLED {
        return text.to_string();
    }

    let mut result = text.to_string();

    // Apply prefix patterns
    for pattern in PREFIX_PATTERNS.iter() {
        result = pattern
            .replace_all(&result, |caps: &regex::Captures| mask_token(&caps[0]))
            .to_string();
    }

    // Apply contextual patterns
    for (i, pattern) in CONTEXTUAL_PATTERNS.iter().enumerate() {
        result = pattern
            .replace_all(&result, |caps: &regex::Captures| {
                match i {
                    0 => {
                        // ENV assignment: keep key, mask value
                        let key = &caps[1];
                        let val = &caps[2];
                        format!("{}={}", key, mask_token(val))
                    }
                    1 => {
                        // JSON field: keep field name, mask value
                        let full = &caps[0];
                        if let Some(m) = caps.get(1) {
                            full.replace(m.as_str(), &mask_token(m.as_str()))
                        } else {
                            mask_token(full)
                        }
                    }
                    2 => {
                        // Authorization header: mask token part
                        let full = &caps[0];
                        if let Some(m) = caps.get(1) {
                            full.replace(m.as_str(), &mask_token(m.as_str()))
                        } else {
                            mask_token(full)
                        }
                    }
                    3 => {
                        // Private key block: replace entirely
                        "[REDACTED PRIVATE KEY]".to_string()
                    }
                    4 => {
                        // Database URI: mask password
                        let full = &caps[0];
                        if let Some(m) = caps.get(1) {
                            full.replace(m.as_str(), &mask_token(m.as_str()))
                        } else {
                            mask_token(full)
                        }
                    }
                    5 => {
                        // Telegram bot token
                        mask_token(&caps[0])
                    }
                    _ => caps[0].to_string(),
                }
            })
            .to_string();
    }

    result
}

/// Check if redaction is currently enabled.
pub fn is_redaction_enabled() -> bool {
    *REDACT_ENABLED
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_short_token() {
        assert_eq!(mask_token("short"), "***");
    }

    #[test]
    fn test_mask_long_token() {
        let token = "sk-abcdefghijklmnopqrstuvwxyz";
        let masked = mask_token(token);
        assert!(masked.starts_with("sk-abc"));
        assert!(masked.ends_with("wxyz"));
        assert!(masked.contains("..."));
    }

    #[test]
    fn test_redact_openai_key() {
        let text = "My key is sk-1234567890abcdefghijklmnop";
        let redacted = redact_sensitive_text(text);
        assert!(!redacted.contains("1234567890abcdefghijklmnop"));
        assert!(redacted.contains("..."));
    }

    #[test]
    fn test_redact_disabled() {
        // When REDACT_ENABLED is true (default), this won't actually test disabled
        // but ensures the function doesn't panic
        let text = "Bearer some-token-value";
        let _ = redact_sensitive_text(text);
    }

    #[test]
    fn test_no_false_positives() {
        let text = "Hello world, this is a normal message with no secrets.";
        assert_eq!(redact_sensitive_text(text), text);
    }
}
