//! API error classification and retry logic.
//!
//! Classifies HTTP errors from LLM providers into actionable categories
//! and provides retry guidance (retryable, wait duration, etc.).

use std::time::Duration;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Broad category of an API error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiErrorKind {
    /// 429 — rate limited, should retry after backoff.
    RateLimit,
    /// 402 / quota-related — billing or quota exceeded.
    QuotaExceeded,
    /// 401 / 403 — authentication or permission failure.
    AuthFailed,
    /// 400 — malformed request.
    InvalidRequest,
    /// 404 — model or endpoint not found.
    ModelNotFound,
    /// 400 with context_length indicator — message too long.
    ContextLength,
    /// 500 — internal server error.
    ServerError,
    /// 502 / 503 — upstream or service unavailable.
    ServiceUnavailable,
    /// Network / connection error (no HTTP status).
    NetworkError,
    /// Request or connection timed out.
    Timeout,
    /// Unrecognised error.
    Unknown,
}

impl std::fmt::Display for ApiErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RateLimit => write!(f, "rate_limit"),
            Self::QuotaExceeded => write!(f, "quota_exceeded"),
            Self::AuthFailed => write!(f, "auth_failed"),
            Self::InvalidRequest => write!(f, "invalid_request"),
            Self::ModelNotFound => write!(f, "model_not_found"),
            Self::ContextLength => write!(f, "context_length"),
            Self::ServerError => write!(f, "server_error"),
            Self::ServiceUnavailable => write!(f, "service_unavailable"),
            Self::NetworkError => write!(f, "network_error"),
            Self::Timeout => write!(f, "timeout"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// A classified API error with retry guidance.
#[derive(Debug, Clone)]
pub struct ClassifiedError {
    pub kind: ApiErrorKind,
    pub status_code: Option<u16>,
    pub message: String,
    pub retryable: bool,
    /// Suggested wait before retrying (if retryable).
    pub retry_after: Duration,
}

impl std::fmt::Display for ClassifiedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(code) = self.status_code {
            write!(f, "[{}] {}: {}", code, self.kind, self.message)
        } else {
            write!(f, "[{}] {}", self.kind, self.message)
        }
    }
}

// ---------------------------------------------------------------------------
// Body content sniffing helpers
// ---------------------------------------------------------------------------

fn body_contains_any(body: &str, patterns: &[&str]) -> bool {
    let lower = body.to_lowercase();
    patterns.iter().any(|p| lower.contains(p))
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// Classify an API error from status code + response body.
///
/// `retry_after_header` is the value of the `Retry-After` header, if present.
pub fn classify_api_error(
    status: u16,
    body: &str,
    retry_after_header: Option<&str>,
) -> ClassifiedError {
    // Parse Retry-After header (seconds or HTTP-date — we only handle seconds).
    let retry_after_secs: f64 = retry_after_header
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);

    match status {
        429 => {
            let wait = if retry_after_secs > 0.0 {
                retry_after_secs
            } else {
                60.0 // default 1 minute backoff
            };
            ClassifiedError {
                kind: ApiErrorKind::RateLimit,
                status_code: Some(429),
                message: extract_error_message(body),
                retryable: true,
                retry_after: Duration::from_secs_f64(wait),
            }
        }
        402 => ClassifiedError {
            kind: ApiErrorKind::QuotaExceeded,
            status_code: Some(402),
            message: extract_error_message(body),
            retryable: false,
            retry_after: Duration::ZERO,
        },
        401 | 403 => ClassifiedError {
            kind: ApiErrorKind::AuthFailed,
            status_code: Some(status),
            message: extract_error_message(body),
            retryable: false,
            retry_after: Duration::ZERO,
        },
        404 => ClassifiedError {
            kind: ApiErrorKind::ModelNotFound,
            status_code: Some(404),
            message: extract_error_message(body),
            retryable: false,
            retry_after: Duration::ZERO,
        },
        400 => {
            // Distinguish context length from generic bad request.
            if body_contains_any(
                body,
                &[
                    "context_length",
                    "context length",
                    "maximum context",
                    "too many tokens",
                    "token limit",
                    "max_tokens",
                    "reduce the length",
                ],
            ) {
                ClassifiedError {
                    kind: ApiErrorKind::ContextLength,
                    status_code: Some(400),
                    message: extract_error_message(body),
                    retryable: false,
                    retry_after: Duration::ZERO,
                }
            } else if body_contains_any(body, &["rate_limit", "rate limit", "too many requests"]) {
                // Some providers return 400 for rate limits.
                ClassifiedError {
                    kind: ApiErrorKind::RateLimit,
                    status_code: Some(400),
                    message: extract_error_message(body),
                    retryable: true,
                    retry_after: Duration::from_secs(30),
                }
            } else if body_contains_any(body, &["quota", "billing", "insufficient_quota", "exceeded"]) {
                ClassifiedError {
                    kind: ApiErrorKind::QuotaExceeded,
                    status_code: Some(400),
                    message: extract_error_message(body),
                    retryable: false,
                    retry_after: Duration::ZERO,
                }
            } else {
                ClassifiedError {
                    kind: ApiErrorKind::InvalidRequest,
                    status_code: Some(400),
                    message: extract_error_message(body),
                    retryable: false,
                    retry_after: Duration::ZERO,
                }
            }
        }
        500 => ClassifiedError {
            kind: ApiErrorKind::ServerError,
            status_code: Some(500),
            message: extract_error_message(body),
            retryable: true,
            retry_after: Duration::from_secs(5),
        },
        502 | 503 => ClassifiedError {
            kind: ApiErrorKind::ServiceUnavailable,
            status_code: Some(status),
            message: extract_error_message(body),
            retryable: true,
            retry_after: Duration::from_secs(10),
        },
        _ if status >= 500 => ClassifiedError {
            kind: ApiErrorKind::ServerError,
            status_code: Some(status),
            message: extract_error_message(body),
            retryable: true,
            retry_after: Duration::from_secs(5),
        },
        _ => ClassifiedError {
            kind: ApiErrorKind::Unknown,
            status_code: Some(status),
            message: extract_error_message(body),
            retryable: false,
            retry_after: Duration::ZERO,
        },
    }
}

/// Classify a network / connection-level error (no HTTP status).
pub fn classify_network_error(err: &str) -> ClassifiedError {
    let lower = err.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        ClassifiedError {
            kind: ApiErrorKind::Timeout,
            status_code: None,
            message: err.to_string(),
            retryable: true,
            retry_after: Duration::from_secs(5),
        }
    } else {
        ClassifiedError {
            kind: ApiErrorKind::NetworkError,
            status_code: None,
            message: err.to_string(),
            retryable: true,
            retry_after: Duration::from_secs(3),
        }
    }
}

/// Whether the error is retryable.
pub fn should_retry(err: &ClassifiedError) -> bool {
    err.retryable
}

/// Suggested wait duration before retrying.
pub fn suggested_wait(err: &ClassifiedError) -> Duration {
    err.retry_after
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract a human-readable error message from a JSON response body.
fn extract_error_message(body: &str) -> String {
    // Try to parse JSON and extract message field.
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(err) = json.get("error") {
            if let Some(msg) = err.get("message").and_then(|m| m.as_str()) {
                return msg.to_string();
            }
            if let Some(msg) = err.as_str() {
                return msg.to_string();
            }
        }
        if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
            return msg.to_string();
        }
    }

    // Fall back to raw body (truncated).
    if body.len() > 200 {
        format!("{}...", &body[..200])
    } else {
        body.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit() {
        let err = classify_api_error(429, r#"{"error": {"message": "Rate limit exceeded"}}"#, None);
        assert_eq!(err.kind, ApiErrorKind::RateLimit);
        assert!(err.retryable);
        assert!(err.retry_after.as_secs() >= 60);
    }

    #[test]
    fn test_context_length() {
        let err = classify_api_error(
            400,
            r#"{"error": {"message": "This model's maximum context length is 4096"}}"#,
            None,
        );
        assert_eq!(err.kind, ApiErrorKind::ContextLength);
        assert!(!err.retryable);
    }

    #[test]
    fn test_auth_failure() {
        let err = classify_api_error(401, r#"{"error": "Invalid API key"}"#, None);
        assert_eq!(err.kind, ApiErrorKind::AuthFailed);
        assert!(!err.retryable);
    }

    #[test]
    fn test_network_timeout() {
        let err = classify_network_error("connection timed out after 30s");
        assert_eq!(err.kind, ApiErrorKind::Timeout);
        assert!(err.retryable);
    }

    #[test]
    fn test_retry_after_header() {
        let err = classify_api_error(429, "{}", Some("120"));
        assert_eq!(err.retry_after.as_secs(), 120);
    }
}
