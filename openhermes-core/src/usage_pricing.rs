//! Comprehensive pricing database with billing route resolution, cost
//! calculation, and token usage normalisation across multiple LLM providers.

use once_cell::sync::Lazy;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Canonical token usage — provider-agnostic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanonicalUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub reasoning_tokens: u64,
    pub request_count: u64,
}

impl CanonicalUsage {
    /// Total prompt tokens (input + cache).
    pub fn prompt_tokens(&self) -> u64 {
        self.input_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    /// Total tokens (prompt + output).
    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens() + self.output_tokens
    }

    /// Accumulate another usage into this one.
    pub fn accumulate(&mut self, other: &CanonicalUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
        self.reasoning_tokens += other.reasoning_tokens;
        self.request_count += other.request_count;
    }
}

/// Identifies the billing route for a request.
#[derive(Debug, Clone)]
pub struct BillingRoute {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    pub billing_mode: String,
}

/// Cost status indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostStatus {
    Actual,
    Estimated,
    Included,
    Unknown,
}

impl std::fmt::Display for CostStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Actual => write!(f, "actual"),
            Self::Estimated => write!(f, "estimated"),
            Self::Included => write!(f, "included"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Where the pricing data came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CostSource {
    OfficialDocsSnapshot,
    ProviderModelsApi,
    UserOverride,
    None,
}

impl std::fmt::Display for CostSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OfficialDocsSnapshot => write!(f, "official_docs_snapshot"),
            Self::ProviderModelsApi => write!(f, "provider_models_api"),
            Self::UserOverride => write!(f, "user_override"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Pricing entry for one (provider, model) pair — costs per 1 M tokens.
#[derive(Debug, Clone)]
pub struct PricingEntry {
    pub input_cost_per_million: Option<Decimal>,
    pub output_cost_per_million: Option<Decimal>,
    pub cache_read_cost_per_million: Option<Decimal>,
    pub cache_write_cost_per_million: Option<Decimal>,
    pub request_cost: Option<Decimal>,
    pub source: CostSource,
    pub source_url: Option<String>,
}

/// Result of a cost estimation.
#[derive(Debug, Clone, Serialize)]
pub struct CostResult {
    pub amount_usd: Option<Decimal>,
    pub status: CostStatus,
    pub source: CostSource,
    pub label: String,
}

// ---------------------------------------------------------------------------
// Pricing database (official docs snapshot)
// ---------------------------------------------------------------------------

type PricingKey = (&'static str, &'static str); // (provider, model)

static PRICING_DB: Lazy<HashMap<PricingKey, PricingEntry>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // Helper to insert a standard entry.
    macro_rules! p {
        ($prov:expr, $model:expr, $in:expr, $out:expr) => {
            m.insert(
                ($prov, $model),
                PricingEntry {
                    input_cost_per_million: Some(Decimal::from_str_exact($in).unwrap()),
                    output_cost_per_million: Some(Decimal::from_str_exact($out).unwrap()),
                    cache_read_cost_per_million: None,
                    cache_write_cost_per_million: None,
                    request_cost: None,
                    source: CostSource::OfficialDocsSnapshot,
                    source_url: None,
                },
            );
        };
        ($prov:expr, $model:expr, $in:expr, $out:expr, $cr:expr, $cw:expr) => {
            m.insert(
                ($prov, $model),
                PricingEntry {
                    input_cost_per_million: Some(Decimal::from_str_exact($in).unwrap()),
                    output_cost_per_million: Some(Decimal::from_str_exact($out).unwrap()),
                    cache_read_cost_per_million: Some(Decimal::from_str_exact($cr).unwrap()),
                    cache_write_cost_per_million: Some(Decimal::from_str_exact($cw).unwrap()),
                    request_cost: None,
                    source: CostSource::OfficialDocsSnapshot,
                    source_url: None,
                },
            );
        };
    }

    // --- Anthropic ---
    p!("anthropic", "claude-opus-4-20250514",   "15.00",  "75.00", "1.50",  "18.75");
    p!("anthropic", "claude-sonnet-4-20250514", "3.00",   "15.00", "0.30",  "3.75");
    p!("anthropic", "claude-3-5-sonnet-20241022", "3.00", "15.00", "0.30",  "3.75");
    p!("anthropic", "claude-3-5-haiku-20241022",  "0.80",  "4.00", "0.08",  "1.00");
    p!("anthropic", "claude-3-opus-20240229",   "15.00",  "75.00", "1.50",  "18.75");
    p!("anthropic", "claude-3-haiku-20240307",  "0.25",   "1.25",  "0.03",  "0.30");

    // --- OpenAI ---
    p!("openai", "gpt-4o",           "2.50",  "10.00");
    p!("openai", "gpt-4o-2024-11-20","2.50",  "10.00");
    p!("openai", "gpt-4o-mini",      "0.15",  "0.60");
    p!("openai", "gpt-4-turbo",      "10.00", "30.00");
    p!("openai", "gpt-4",            "30.00", "60.00");
    p!("openai", "gpt-3.5-turbo",    "0.50",  "1.50");
    p!("openai", "o3",               "10.00", "40.00");
    p!("openai", "o3-mini",          "1.10",  "4.40");
    p!("openai", "o1",               "15.00", "60.00");
    p!("openai", "o1-mini",          "3.00",  "12.00");
    p!("openai", "o1-preview",       "15.00", "60.00");

    // --- DeepSeek ---
    p!("deepseek", "deepseek-chat",      "0.27",  "1.10");
    p!("deepseek", "deepseek-reasoner",  "0.55",  "2.19");

    // --- Google ---
    p!("google", "gemini-2.0-flash",    "0.10",  "0.40");
    p!("google", "gemini-2.0-pro",      "1.25",  "10.00");
    p!("google", "gemini-1.5-flash",    "0.075", "0.30");
    p!("google", "gemini-1.5-pro",      "1.25",  "5.00");

    // --- Meta (via providers) ---
    p!("meta", "llama-3.3-70b",     "0.60",  "0.60");
    p!("meta", "llama-3.1-405b",    "3.00",  "3.00");
    p!("meta", "llama-3.1-70b",     "0.60",  "0.60");
    p!("meta", "llama-3.1-8b",      "0.10",  "0.10");

    // --- Mistral ---
    p!("mistral", "mistral-large-latest",  "2.00",  "6.00");
    p!("mistral", "mistral-medium-latest", "2.70",  "8.10");
    p!("mistral", "mistral-small-latest",  "0.20",  "0.60");
    p!("mistral", "codestral-latest",      "0.30",  "0.90");

    m
});

// ---------------------------------------------------------------------------
// Billing route resolution
// ---------------------------------------------------------------------------

/// Resolve the billing route for a model + provider + base_url combination.
pub fn resolve_billing_route(
    model: &str,
    provider: Option<&str>,
    base_url: Option<&str>,
) -> BillingRoute {
    let base = base_url.unwrap_or("");
    let lower_base = base.to_lowercase();

    // Infer provider from model name or base_url.
    let inferred_provider = if let Some(p) = provider {
        p.to_string()
    } else if model.starts_with("claude") {
        "anthropic".to_string()
    } else if model.starts_with("gpt-") || model.starts_with("o1") || model.starts_with("o3") {
        "openai".to_string()
    } else if model.starts_with("gemini") {
        "google".to_string()
    } else if model.starts_with("deepseek") {
        "deepseek".to_string()
    } else if model.starts_with("llama") {
        "meta".to_string()
    } else if model.starts_with("mistral") || model.starts_with("codestral") {
        "mistral".to_string()
    } else {
        "unknown".to_string()
    };

    // Determine billing mode.
    let billing_mode = if lower_base.contains("openrouter") {
        "openrouter"
    } else if lower_base.contains("localhost") || lower_base.contains("127.0.0.1") {
        "local"
    } else {
        "direct"
    };

    BillingRoute {
        provider: inferred_provider,
        model: model.to_string(),
        base_url: base.to_string(),
        billing_mode: billing_mode.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Pricing lookup
// ---------------------------------------------------------------------------

/// Look up pricing for a provider + model.
fn get_pricing_entry(provider: &str, model: &str) -> Option<&'static PricingEntry> {
    // Exact match first.
    if let Some(entry) = PRICING_DB.get(&(provider, model)) {
        // SAFETY: Lazy keys are &'static str, but we have dynamic strings.
        // We need to iterate instead.
        let _ = entry;
    }

    // Iterate for dynamic strings.
    for ((p, m), entry) in PRICING_DB.iter() {
        if *p == provider && *m == model {
            return Some(entry);
        }
    }

    // Try prefix matching (e.g. "gpt-4o-2024-08-06" matches "gpt-4o").
    let mut best: Option<(&'static PricingEntry, usize)> = None;
    for ((p, m), entry) in PRICING_DB.iter() {
        if *p == provider && model.starts_with(m) {
            let len = m.len();
            if best.is_none() || len > best.unwrap().1 {
                best = Some((entry, len));
            }
        }
    }
    best.map(|(e, _)| e)
}

/// Check if a model has known pricing.
pub fn has_known_pricing(model: &str, provider: Option<&str>, base_url: Option<&str>) -> bool {
    let route = resolve_billing_route(model, provider, base_url);
    if route.billing_mode == "local" {
        return true; // local models are free
    }
    get_pricing_entry(&route.provider, &route.model).is_some()
}

// ---------------------------------------------------------------------------
// Cost estimation
// ---------------------------------------------------------------------------

const ONE_MILLION: Decimal = Decimal::from_parts(1000000, 0, 0, false, 0);

/// Estimate the cost of a request.
pub fn estimate_cost(
    model: &str,
    usage: &CanonicalUsage,
    provider: Option<&str>,
    base_url: Option<&str>,
) -> CostResult {
    let route = resolve_billing_route(model, provider, base_url);

    // Local models are free.
    if route.billing_mode == "local" {
        return CostResult {
            amount_usd: Some(Decimal::ZERO),
            status: CostStatus::Included,
            source: CostSource::None,
            label: "local (free)".to_string(),
        };
    }

    let entry = match get_pricing_entry(&route.provider, &route.model) {
        Some(e) => e,
        None => {
            return CostResult {
                amount_usd: None,
                status: CostStatus::Unknown,
                source: CostSource::None,
                label: format!("unknown pricing for {}/{}", route.provider, route.model),
            };
        }
    };

    let mut total = Decimal::ZERO;

    // Input tokens
    if let Some(rate) = entry.input_cost_per_million {
        total += rate * Decimal::from(usage.input_tokens) / ONE_MILLION;
    }

    // Output tokens
    if let Some(rate) = entry.output_cost_per_million {
        total += rate * Decimal::from(usage.output_tokens) / ONE_MILLION;
    }

    // Cache read tokens
    if let Some(rate) = entry.cache_read_cost_per_million {
        total += rate * Decimal::from(usage.cache_read_tokens) / ONE_MILLION;
    }

    // Cache write tokens
    if let Some(rate) = entry.cache_write_cost_per_million {
        total += rate * Decimal::from(usage.cache_write_tokens) / ONE_MILLION;
    }

    // Per-request cost
    if let Some(rate) = entry.request_cost {
        total += rate * Decimal::from(usage.request_count);
    }

    CostResult {
        amount_usd: Some(total),
        status: CostStatus::Estimated,
        source: entry.source.clone(),
        label: format!("{}/{}", route.provider, route.model),
    }
}

// ---------------------------------------------------------------------------
// Usage normalisation
// ---------------------------------------------------------------------------

/// Normalise a raw API response usage object into canonical form.
///
/// Handles OpenAI Chat, Anthropic, and Codex Response formats.
pub fn normalize_usage(raw: &serde_json::Value) -> CanonicalUsage {
    let get_u64 = |key: &str| -> u64 {
        raw.get(key).and_then(|v| v.as_u64()).unwrap_or(0)
    };

    let input_tokens = get_u64("prompt_tokens")
        .max(get_u64("input_tokens"));

    let output_tokens = get_u64("completion_tokens")
        .max(get_u64("output_tokens"));

    // Anthropic cache fields
    let cache_read_tokens = get_u64("cache_read_input_tokens")
        .max(get_u64("cache_read_tokens"));

    let cache_write_tokens = get_u64("cache_creation_input_tokens")
        .max(get_u64("cache_write_tokens"));

    // OpenAI reasoning tokens (o1/o3 models)
    let reasoning_tokens = raw
        .get("completion_tokens_details")
        .and_then(|d| d.get("reasoning_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    CanonicalUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
        reasoning_tokens,
        request_count: 1,
    }
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/// Format a cost result as a human-readable string.
pub fn format_cost(cost: &CostResult) -> String {
    match cost.amount_usd {
        Some(amount) if amount == Decimal::ZERO => "free".to_string(),
        Some(amount) => {
            if amount < Decimal::from_str_exact("0.01").unwrap() {
                format!("<$0.01 ({})", cost.status)
            } else {
                format!("${:.4} ({})", amount, cost.status)
            }
        }
        None => format!("unknown ({})", cost.label),
    }
}

/// Format a duration in seconds into a compact string.
pub fn format_duration_compact(secs: f64) -> String {
    if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
    } else if secs < 60.0 {
        format!("{:.1}s", secs)
    } else if secs < 3600.0 {
        let mins = (secs / 60.0).floor();
        let remaining = secs - mins * 60.0;
        format!("{:.0}m{:.0}s", mins, remaining)
    } else {
        let hours = (secs / 3600.0).floor();
        let remaining = secs - hours * 3600.0;
        let mins = (remaining / 60.0).floor();
        format!("{:.0}h{:.0}m", hours, mins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_openai() {
        let route = resolve_billing_route("gpt-4o", None, None);
        assert_eq!(route.provider, "openai");
        assert_eq!(route.billing_mode, "direct");
    }

    #[test]
    fn test_resolve_anthropic() {
        let route = resolve_billing_route("claude-3-5-sonnet-20241022", None, None);
        assert_eq!(route.provider, "anthropic");
    }

    #[test]
    fn test_resolve_local() {
        let route = resolve_billing_route("llama-3", None, Some("http://localhost:11434"));
        assert_eq!(route.billing_mode, "local");
    }

    #[test]
    fn test_estimate_cost_gpt4o() {
        let usage = CanonicalUsage {
            input_tokens: 1000,
            output_tokens: 500,
            request_count: 1,
            ..Default::default()
        };
        let result = estimate_cost("gpt-4o", &usage, Some("openai"), None);
        assert!(result.amount_usd.is_some());
        assert_eq!(result.status, CostStatus::Estimated);
    }

    #[test]
    fn test_estimate_cost_local() {
        let usage = CanonicalUsage {
            input_tokens: 1000,
            output_tokens: 500,
            request_count: 1,
            ..Default::default()
        };
        let result = estimate_cost("llama", &usage, None, Some("http://localhost:11434"));
        assert_eq!(result.amount_usd, Some(Decimal::ZERO));
        assert_eq!(result.status, CostStatus::Included);
    }

    #[test]
    fn test_normalize_openai_usage() {
        let raw = serde_json::json!({
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        });
        let usage = normalize_usage(&raw);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
    }

    #[test]
    fn test_normalize_anthropic_usage() {
        let raw = serde_json::json!({
            "input_tokens": 200,
            "output_tokens": 100,
            "cache_read_input_tokens": 50,
            "cache_creation_input_tokens": 30
        });
        let usage = normalize_usage(&raw);
        assert_eq!(usage.input_tokens, 200);
        assert_eq!(usage.cache_read_tokens, 50);
        assert_eq!(usage.cache_write_tokens, 30);
    }

    #[test]
    fn test_has_known_pricing() {
        assert!(has_known_pricing("gpt-4o", Some("openai"), None));
        assert!(has_known_pricing("llama", None, Some("http://localhost:11434")));
        assert!(!has_known_pricing("some-random-model", None, None));
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration_compact(0.5), "500ms");
        assert_eq!(format_duration_compact(45.0), "45.0s");
        assert_eq!(format_duration_compact(125.0), "2m5s");
        assert_eq!(format_duration_compact(3700.0), "1h1m");
    }

    #[test]
    fn test_canonical_usage_accumulate() {
        let mut total = CanonicalUsage::default();
        let a = CanonicalUsage { input_tokens: 100, output_tokens: 50, request_count: 1, ..Default::default() };
        let b = CanonicalUsage { input_tokens: 200, output_tokens: 80, request_count: 1, ..Default::default() };
        total.accumulate(&a);
        total.accumulate(&b);
        assert_eq!(total.input_tokens, 300);
        assert_eq!(total.output_tokens, 130);
        assert_eq!(total.request_count, 2);
    }
}
