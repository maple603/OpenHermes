//! Shared constants for OpenHermes Agent.
//!
//! Import-safe module with no heavy dependencies — can be imported from anywhere
//! without risk of circular imports.

use std::path::PathBuf;

// ============================================================================
// API Endpoints
// ============================================================================

/// OpenRouter API base URL
pub static OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// OpenRouter models endpoint
pub static OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

/// OpenRouter chat completions endpoint
pub static OPENROUTER_CHAT_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// AI Gateway base URL
pub static AI_GATEWAY_BASE_URL: &str = "https://ai-gateway.vercel.sh/v1";

/// AI Gateway models endpoint
pub static AI_GATEWAY_MODELS_URL: &str = "https://ai-gateway.vercel.sh/v1/models";

/// AI Gateway chat completions endpoint
pub static AI_GATEWAY_CHAT_URL: &str = "https://ai-gateway.vercel.sh/v1/chat/completions";

/// Nous Research API base URL
pub static NOUS_API_BASE_URL: &str = "https://inference-api.nousresearch.com/v1";

/// Nous Research chat completions endpoint
pub static NOUS_API_CHAT_URL: &str = "https://inference-api.nousresearch.com/v1/chat/completions";

// ============================================================================
// Reasoning Effort Levels
// ============================================================================

/// Valid reasoning effort levels
pub const VALID_REASONING_EFFORTS: &[&str] = &["xhigh", "high", "medium", "low", "minimal"];

/// Reasoning effort configuration
#[derive(Debug, Clone)]
pub struct ReasoningEffortConfig {
    pub enabled: bool,
    pub effort: Option<String>,
}

/// Parse a reasoning effort level into a config struct
pub fn parse_reasoning_effort(effort: &str) -> Option<ReasoningEffortConfig> {
    let effort = effort.trim();
    if effort.is_empty() {
        return None;
    }

    let effort_lower = effort.to_lowercase();
    if effort_lower == "none" {
        return Some(ReasoningEffortConfig {
            enabled: false,
            effort: None,
        });
    }

    if VALID_REASONING_EFFORTS.contains(&effort_lower.as_str()) {
        return Some(ReasoningEffortConfig {
            enabled: true,
            effort: Some(effort_lower),
        });
    }

    None
}

// ============================================================================
// Hermes Home Directory
// ============================================================================

/// Get the Hermes home directory (default: ~/.openhermes).
///
/// Reads HERMES_HOME env var, falls back to ~/.openhermes.
/// This is the single source of truth — all other copies should import this.
pub fn get_hermes_home() -> PathBuf {
    std::env::var("HERMES_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".openhermes")
        })
}

/// Get the optional skills directory, honoring package-manager wrappers.
///
/// Packaged installs may ship `optional-skills` outside the Python package
/// tree and expose it via `HERMES_OPTIONAL_SKILLS`.
pub fn get_optional_skills_dir(default: Option<PathBuf>) -> PathBuf {
    if let Ok(override_dir) = std::env::var("HERMES_OPTIONAL_SKILLS") {
        if !override_dir.trim().is_empty() {
            return PathBuf::from(override_dir);
        }
    }

    default.unwrap_or_else(|| get_hermes_home().join("optional-skills"))
}

/// Resolve a Hermes subdirectory with backward compatibility.
///
/// New installs get the consolidated layout (e.g. `cache/images`).
/// Existing installs that already have the old path (e.g. `image_cache`)
/// keep using it — no migration required.
pub fn get_hermes_dir(new_subpath: &str, old_name: &str) -> PathBuf {
    let home = get_hermes_home();
    let old_path = home.join(old_name);
    if old_path.exists() {
        old_path
    } else {
        home.join(new_subpath)
    }
}

/// Return a user-friendly display string for the current HERMES_HOME.
///
/// Uses `~/` shorthand for readability:
/// - default: `~/.openhermes`
/// - profile: `~/.openhermes/profiles/coder`
/// - custom: `/opt/hermes-custom`
///
/// Use this in **user-facing** print/log messages instead of hardcoding
/// `~/.openhermes`. For code that needs a real `Path`, use [`get_hermes_home`] instead.
pub fn display_hermes_home() -> String {
    let home = get_hermes_home();
    if let Some(home_dir) = dirs::home_dir() {
        if let Ok(rel) = home.strip_prefix(&home_dir) {
            return format!("~/{}", rel.display());
        }
    }
    home.display().to_string()
}

// ============================================================================
// Default Values
// ============================================================================

/// Default maximum iterations for agent loop
pub const DEFAULT_MAX_ITERATIONS: usize = 90;

/// Default maximum iterations for subagent delegation
pub const DEFAULT_DELEGATION_MAX_ITERATIONS: usize = 50;

/// Default tool execution timeout in seconds
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 300;

/// Default context compression threshold (tokens)
pub const DEFAULT_COMPRESSION_THRESHOLD: usize = 80_000;

/// Default target context size after compression (tokens)
pub const DEFAULT_TARGET_CONTEXT_SIZE: usize = 40_000;
