//! Helpers for optional cheap-vs-strong model routing.
//!
//! When the user's message is simple enough (short, no code, no complex keywords),
//! route to a cheaper / faster model to save cost and latency. Complex queries
//! always go to the primary (strong) model.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Routing configuration — typically populated from `HermesConfig`.
#[derive(Debug, Clone)]
pub struct RoutingConfig {
    /// Primary (strong) model, e.g. `gpt-4o`.
    pub primary_model: String,
    /// Cheap / fast model for simple queries, e.g. `gpt-4o-mini`.
    pub cheap_model: String,
    /// Whether smart routing is enabled.
    pub enabled: bool,
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self {
            primary_model: "gpt-4o".to_string(),
            cheap_model: "gpt-4o-mini".to_string(),
            enabled: false,
        }
    }
}

/// Result of a routing decision.
#[derive(Debug, Clone)]
pub struct RouteDecision {
    /// The model to use for this turn.
    pub model: String,
    /// Whether the cheap model was selected.
    pub is_cheap: bool,
    /// Human-readable reason for the decision.
    pub reason: String,
}

// ---------------------------------------------------------------------------
// Keyword set
// ---------------------------------------------------------------------------

static COMPLEX_KEYWORDS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "debug", "debugging", "implement", "implementation", "refactor",
        "patch", "traceback", "stacktrace", "exception", "error",
        "analyze", "analysis", "investigate", "architecture", "design",
        "compare", "benchmark", "optimize", "optimise", "review",
        "terminal", "shell", "tool", "tools", "pytest", "test", "tests",
        "plan", "planning", "delegate", "subagent", "cron", "docker",
        "kubernetes", "deploy", "migration", "database", "schema",
        "security", "vulnerability", "performance", "profiling",
        "concurrency", "async", "parallel",
    ]
    .into_iter()
    .collect()
});

static URL_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https?://|www\.").unwrap()
});

// ---------------------------------------------------------------------------
// Heuristics
// ---------------------------------------------------------------------------

/// Determine whether a message is "simple" enough for the cheap model.
fn is_simple_message(message: &str) -> (bool, &'static str) {
    let trimmed = message.trim();

    // Length checks
    if trimmed.len() > 160 {
        return (false, "message too long (>160 chars)");
    }

    let word_count = trimmed.split_whitespace().count();
    if word_count > 28 {
        return (false, "too many words (>28)");
    }

    let newline_count = trimmed.matches('\n').count();
    if newline_count >= 2 {
        return (false, "multiple newlines (>=2)");
    }

    // Code detection
    if trimmed.contains('`') {
        return (false, "contains code (backticks)");
    }
    if trimmed.contains("```") {
        return (false, "contains code block");
    }

    // URL detection
    if URL_RE.is_match(trimmed) {
        return (false, "contains URL");
    }

    // Complex keyword detection
    let lower = trimmed.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .collect();

    for word in &words {
        if COMPLEX_KEYWORDS.contains(word) {
            return (false, "contains complex keyword");
        }
    }

    (true, "simple message")
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Choose which model to route this message to.
///
/// Returns a `RouteDecision` indicating the chosen model and reasoning.
/// If routing is disabled or the message is complex, always returns the
/// primary model.
pub fn choose_model_route(message: &str, config: &RoutingConfig) -> RouteDecision {
    if !config.enabled {
        return RouteDecision {
            model: config.primary_model.clone(),
            is_cheap: false,
            reason: "routing disabled".to_string(),
        };
    }

    if config.cheap_model.is_empty() || config.cheap_model == config.primary_model {
        return RouteDecision {
            model: config.primary_model.clone(),
            is_cheap: false,
            reason: "no cheap model configured".to_string(),
        };
    }

    let (simple, reason) = is_simple_message(message);
    if simple {
        RouteDecision {
            model: config.cheap_model.clone(),
            is_cheap: true,
            reason: format!("routed to cheap model: {}", reason),
        }
    } else {
        RouteDecision {
            model: config.primary_model.clone(),
            is_cheap: false,
            reason: format!("routed to primary model: {}", reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RoutingConfig {
        RoutingConfig {
            primary_model: "gpt-4o".to_string(),
            cheap_model: "gpt-4o-mini".to_string(),
            enabled: true,
        }
    }

    #[test]
    fn test_simple_greeting() {
        let decision = choose_model_route("Hello!", &test_config());
        assert!(decision.is_cheap);
        assert_eq!(decision.model, "gpt-4o-mini");
    }

    #[test]
    fn test_complex_with_code() {
        let decision = choose_model_route("Fix the `parse_config` function", &test_config());
        assert!(!decision.is_cheap);
        assert_eq!(decision.model, "gpt-4o");
    }

    #[test]
    fn test_complex_keyword() {
        let decision = choose_model_route("Debug this issue", &test_config());
        assert!(!decision.is_cheap);
    }

    #[test]
    fn test_long_message() {
        let long_msg = "a ".repeat(100);
        let decision = choose_model_route(&long_msg, &test_config());
        assert!(!decision.is_cheap);
    }

    #[test]
    fn test_disabled_routing() {
        let config = RoutingConfig {
            enabled: false,
            ..test_config()
        };
        let decision = choose_model_route("Hello!", &config);
        assert!(!decision.is_cheap);
        assert_eq!(decision.model, "gpt-4o");
    }

    #[test]
    fn test_url_detection() {
        let decision = choose_model_route("Check https://example.com", &test_config());
        assert!(!decision.is_cheap);
    }
}
