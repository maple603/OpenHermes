//! Rate limit tracking for inference API responses.
//!
//! Captures `x-ratelimit-*` headers from provider responses and provides
//! formatted display for the /usage slash command.
//!
//! Header schema (12 headers):
//!   x-ratelimit-limit-requests          RPM cap
//!   x-ratelimit-limit-requests-1h       RPH cap
//!   x-ratelimit-limit-tokens            TPM cap
//!   x-ratelimit-limit-tokens-1h         TPH cap
//!   x-ratelimit-remaining-requests      requests left in minute window
//!   x-ratelimit-remaining-requests-1h   requests left in hour window
//!   x-ratelimit-remaining-tokens        tokens left in minute window
//!   x-ratelimit-remaining-tokens-1h     tokens left in hour window
//!   x-ratelimit-reset-requests          seconds until minute request window resets
//!   x-ratelimit-reset-requests-1h       seconds until hour request window resets
//!   x-ratelimit-reset-tokens            seconds until minute token window resets
//!   x-ratelimit-reset-tokens-1h         seconds until hour token window resets

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// One rate-limit window (e.g. requests per minute).
#[derive(Debug, Clone, Default)]
pub struct RateLimitBucket {
    pub limit: u64,
    pub remaining: u64,
    pub reset_seconds: f64,
    /// `SystemTime` epoch seconds when this was captured.
    pub captured_at: f64,
}

impl RateLimitBucket {
    /// Number of units consumed in this window.
    pub fn used(&self) -> u64 {
        self.limit.saturating_sub(self.remaining)
    }

    /// Percentage of the window consumed (0.0 – 100.0).
    pub fn usage_pct(&self) -> f64 {
        if self.limit == 0 {
            return 0.0;
        }
        (self.used() as f64 / self.limit as f64) * 100.0
    }

    /// Seconds remaining until the window resets, accounting for elapsed time.
    pub fn remaining_seconds_now(&self) -> f64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let elapsed = now - self.captured_at;
        (self.reset_seconds - elapsed).max(0.0)
    }

    /// Whether this bucket has any data.
    pub fn has_data(&self) -> bool {
        self.limit > 0 || self.remaining > 0
    }
}

/// Aggregated rate limit state across all four windows.
#[derive(Debug, Clone, Default)]
pub struct RateLimitState {
    pub requests_min: RateLimitBucket,
    pub requests_hour: RateLimitBucket,
    pub tokens_min: RateLimitBucket,
    pub tokens_hour: RateLimitBucket,
    pub provider: String,
}

impl RateLimitState {
    /// Whether any bucket has data.
    pub fn has_data(&self) -> bool {
        self.requests_min.has_data()
            || self.requests_hour.has_data()
            || self.tokens_min.has_data()
            || self.tokens_hour.has_data()
    }

    /// Age in seconds since the most recently captured bucket.
    pub fn age_seconds(&self) -> f64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let latest = [
            self.requests_min.captured_at,
            self.requests_hour.captured_at,
            self.tokens_min.captured_at,
            self.tokens_hour.captured_at,
        ]
        .into_iter()
        .fold(0.0_f64, f64::max);
        if latest == 0.0 {
            return 0.0;
        }
        now - latest
    }
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse rate limit state from HTTP response headers.
///
/// Accepts a generic map of header-name → header-value (both as strings).
/// Header names are matched case-insensitively.
pub fn parse_rate_limit_headers(headers: &HashMap<String, String>) -> RateLimitState {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    // Lowercase all keys for case-insensitive lookup.
    let lc: HashMap<String, &str> = headers
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.as_str()))
        .collect();

    let get_u64 = |key: &str| -> u64 {
        lc.get(key)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let get_f64 = |key: &str| -> f64 {
        lc.get(key)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };

    RateLimitState {
        requests_min: RateLimitBucket {
            limit: get_u64("x-ratelimit-limit-requests"),
            remaining: get_u64("x-ratelimit-remaining-requests"),
            reset_seconds: get_f64("x-ratelimit-reset-requests"),
            captured_at: now,
        },
        requests_hour: RateLimitBucket {
            limit: get_u64("x-ratelimit-limit-requests-1h"),
            remaining: get_u64("x-ratelimit-remaining-requests-1h"),
            reset_seconds: get_f64("x-ratelimit-reset-requests-1h"),
            captured_at: now,
        },
        tokens_min: RateLimitBucket {
            limit: get_u64("x-ratelimit-limit-tokens"),
            remaining: get_u64("x-ratelimit-remaining-tokens"),
            reset_seconds: get_f64("x-ratelimit-reset-tokens"),
            captured_at: now,
        },
        tokens_hour: RateLimitBucket {
            limit: get_u64("x-ratelimit-limit-tokens-1h"),
            remaining: get_u64("x-ratelimit-remaining-tokens-1h"),
            reset_seconds: get_f64("x-ratelimit-reset-tokens-1h"),
            captured_at: now,
        },
        provider: String::new(),
    }
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

fn progress_bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let warning = if pct > 80.0 { "!" } else { "" };
    format!("[{}{}] {:.0}%{}", "█".repeat(filled), "░".repeat(empty), pct, warning)
}

fn format_bucket(label: &str, bucket: &RateLimitBucket) -> String {
    if !bucket.has_data() {
        return String::new();
    }
    let bar = progress_bar(bucket.usage_pct(), 20);
    let reset = bucket.remaining_seconds_now();
    format!(
        "  {:<20} {} ({}/{}) resets in {:.0}s",
        label,
        bar,
        bucket.used(),
        bucket.limit,
        reset
    )
}

/// Format rate limit state for terminal display with progress bars.
pub fn format_rate_limit_display(state: &RateLimitState) -> String {
    if !state.has_data() {
        return "No rate limit data available.".to_string();
    }

    let mut lines = Vec::new();
    let header = if state.provider.is_empty() {
        "Rate Limits:".to_string()
    } else {
        format!("Rate Limits ({}):", state.provider)
    };
    lines.push(header);

    let buckets = [
        ("Requests/min", &state.requests_min),
        ("Requests/hour", &state.requests_hour),
        ("Tokens/min", &state.tokens_min),
        ("Tokens/hour", &state.tokens_hour),
    ];

    for (label, bucket) in &buckets {
        let line = format_bucket(label, bucket);
        if !line.is_empty() {
            lines.push(line);
        }
    }

    if lines.len() == 1 {
        return "No rate limit data available.".to_string();
    }

    let age = state.age_seconds();
    if age > 0.0 {
        lines.push(format!("  (captured {:.0}s ago)", age));
    }

    lines.join("\n")
}

/// One-line compact format for status bars and gateway messages.
pub fn format_rate_limit_compact(state: &RateLimitState) -> String {
    if !state.has_data() {
        return String::new();
    }

    let mut parts = Vec::new();
    if state.requests_min.has_data() {
        parts.push(format!(
            "RPM:{}/{}",
            state.requests_min.used(),
            state.requests_min.limit
        ));
    }
    if state.tokens_min.has_data() {
        parts.push(format!(
            "TPM:{}/{}",
            state.tokens_min.used(),
            state.tokens_min.limit
        ));
    }
    if state.requests_hour.has_data() {
        parts.push(format!(
            "RPH:{}/{}",
            state.requests_hour.used(),
            state.requests_hour.limit
        ));
    }
    if state.tokens_hour.has_data() {
        parts.push(format!(
            "TPH:{}/{}",
            state.tokens_hour.used(),
            state.tokens_hour.limit
        ));
    }

    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_usage() {
        let bucket = RateLimitBucket {
            limit: 100,
            remaining: 75,
            reset_seconds: 60.0,
            captured_at: 0.0,
        };
        assert_eq!(bucket.used(), 25);
        assert!((bucket.usage_pct() - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_parse_headers() {
        let mut headers = HashMap::new();
        headers.insert("x-ratelimit-limit-requests".to_string(), "60".to_string());
        headers.insert("x-ratelimit-remaining-requests".to_string(), "55".to_string());
        headers.insert("x-ratelimit-reset-requests".to_string(), "45".to_string());

        let state = parse_rate_limit_headers(&headers);
        assert_eq!(state.requests_min.limit, 60);
        assert_eq!(state.requests_min.remaining, 55);
        assert!(state.has_data());
    }

    #[test]
    fn test_compact_format() {
        let state = RateLimitState {
            requests_min: RateLimitBucket {
                limit: 60,
                remaining: 55,
                ..Default::default()
            },
            tokens_min: RateLimitBucket {
                limit: 100000,
                remaining: 90000,
                ..Default::default()
            },
            ..Default::default()
        };
        let compact = format_rate_limit_compact(&state);
        assert!(compact.contains("RPM:5/60"));
        assert!(compact.contains("TPM:10000/100000"));
    }
}
