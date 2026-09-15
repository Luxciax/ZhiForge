use serde::Serialize;
use serde_json::Value;
use std::fmt;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AiErrorKind {
    Auth,
    RateLimit,
    Timeout,
    Network,
    Provider,
    MalformedStream,
    EmptyResponse,
    Interrupted,
    Config,
    AllProvidersFailed,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AiRuntimeError {
    pub kind: AiErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    pub retryable: bool,
    pub partial: bool,
}

impl AiRuntimeError {
    pub fn new(kind: AiErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            status: None,
            retryable: false,
            partial: false,
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::Config, message)
    }

    pub fn malformed(message: impl Into<String>) -> Self {
        Self::new(AiErrorKind::MalformedStream, message)
    }

    pub fn empty() -> Self {
        Self::new(AiErrorKind::EmptyResponse, "model returned no text content")
    }

    pub fn idle_timeout() -> Self {
        Self {
            kind: AiErrorKind::Timeout,
            message: "stream idle timed out while waiting for provider data".into(),
            status: None,
            retryable: true,
            partial: false,
        }
    }

    pub fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self {
                kind: AiErrorKind::Timeout,
                message: "request timed out".into(),
                status: None,
                retryable: true,
                partial: false,
            }
        } else {
            Self {
                kind: AiErrorKind::Network,
                message: if error.is_connect() {
                    format!("failed to connect to provider: {error}")
                } else {
                    error.to_string()
                },
                status: None,
                retryable: true,
                partial: false,
            }
        }
    }

    pub fn from_http(status: u16, body: &str) -> Self {
        let message = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .or_else(|| value.get("message").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .unwrap_or_else(|| body.trim().chars().take(600).collect());
        let kind = match status {
            401 | 403 => AiErrorKind::Auth,
            408 => AiErrorKind::Timeout,
            429 => AiErrorKind::RateLimit,
            _ => AiErrorKind::Provider,
        };
        Self {
            kind,
            message: format!("provider returned HTTP {status}: {message}"),
            status: Some(status),
            retryable: matches!(status, 408 | 425 | 429 | 500 | 502 | 503 | 504),
            partial: false,
        }
    }

    pub fn into_partial(mut self) -> Self {
        self.kind = AiErrorKind::Interrupted;
        self.message = format!("stream interrupted; partial result was kept: {}", self.message);
        self.retryable = false;
        self.partial = true;
        self
    }

    pub fn all_providers_failed(failures: Vec<String>) -> Self {
        let message = if failures.is_empty() {
            "all routed providers were unavailable".to_string()
        } else {
            format!(
                "all routed providers failed before output: {}",
                failures.join(" | ")
            )
        };
        Self::new(AiErrorKind::AllProvidersFailed, message)
    }
}

impl fmt::Display for AiRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for AiRuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_errors_are_classified() {
        assert_eq!(AiRuntimeError::from_http(401, "unauthorized").kind, AiErrorKind::Auth);
        let rate_limit = AiRuntimeError::from_http(429, "slow down");
        assert_eq!(rate_limit.kind, AiErrorKind::RateLimit);
        assert!(rate_limit.retryable);
        assert_eq!(AiRuntimeError::from_http(400, "bad request").kind, AiErrorKind::Provider);
    }

    #[test]
    fn partial_error_preserves_status_and_sets_interrupted_kind() {
        let error = AiRuntimeError::from_http(503, "down").into_partial();
        assert_eq!(error.kind, AiErrorKind::Interrupted);
        assert_eq!(error.status, Some(503));
        assert!(error.partial);
        assert!(!error.retryable);
    }

    #[test]
    fn idle_timeout_is_retryable_until_output_exists() {
        let error = AiRuntimeError::idle_timeout();
        assert_eq!(error.kind, AiErrorKind::Timeout);
        assert!(error.retryable);
        assert!(!error.partial);
    }
}
