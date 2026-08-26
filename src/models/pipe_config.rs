//! Typed view over a pipe's `config` JSON blob.
//!
//! `PipeTemplate.config` (and `PipeInstance.config_override`) is an untyped
//! `JsonValue` today — it only ever carried `{"retry_count": 3}`. `PipeConfig`
//! gives the resilience/lifecycle settings a typed home while staying
//! **backward-compatible**: parsing is lenient (unknown keys such as the legacy
//! `retry_count` are ignored, not rejected), absence falls back to defaults, and
//! writing merges *over* the existing blob so unrelated keys are preserved.
//!
//! See `config/docs/PIPE_IAC_AND_RESILIENCE_PLAN.md` (Phase 0 + §4).

use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;

use crate::models::agent_protocol::RetryPolicy;

fn default_notify_method() -> String {
    "POST".to_string()
}

/// Where a pipe's success/failure notification is delivered. Also reused by the
/// (future) `monitoring.alerts` dispatch — see plan §10.
///
/// Serde is externally tagged, matching the declarative schema:
/// `{ "pipe": "oncall-notify" }` or `{ "notify": { "url": "...", "method": "POST" } }`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HandlerRef {
    /// Run another declared pipe by name.
    Pipe(String),
    /// Deliver to an HTTP endpoint (e.g. an ntfy topic).
    Notify {
        url: String,
        #[serde(default = "default_notify_method")]
        method: String,
    },
}

/// Typed resilience + lifecycle settings for a pipe, serialized into the pipe's
/// `config` JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct PipeConfig {
    /// Retry policy for delivery. Absent → engine default (3 / 1000ms / 30000ms).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryPolicy>,
    /// Handler fired after retries are exhausted (delivery failed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_failure: Option<HandlerRef>,
    /// Handler fired after a successful delivery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_success: Option<HandlerRef>,
}

impl PipeConfig {
    /// Parse a `PipeConfig` from a pipe's optional `config` blob. Lenient:
    /// unknown/legacy keys are ignored and a missing or unparseable config
    /// yields defaults, so existing pipes (e.g. `{"retry_count": 3}`) keep
    /// working unchanged.
    pub fn from_value(config: &Option<JsonValue>) -> Self {
        config
            .as_ref()
            .and_then(|value| serde_json::from_value::<PipeConfig>(value.clone()).ok())
            .unwrap_or_default()
    }

    /// Merge these typed settings *over* an existing config blob, preserving any
    /// unrelated keys already present (legacy `retry_count`, custom fields, …).
    /// Returns a JSON object suitable for `PipeTemplate.config`.
    pub fn merge_into(&self, base: Option<JsonValue>) -> JsonValue {
        let mut obj = match base {
            Some(JsonValue::Object(map)) => map,
            _ => serde_json::Map::new(),
        };
        if let Ok(JsonValue::Object(mine)) = serde_json::to_value(self) {
            for (key, value) in mine {
                obj.insert(key, value);
            }
        }
        JsonValue::Object(obj)
    }

    /// Effective retry policy for the delivery step (falls back to the engine
    /// default when unset). This is the value #4a feeds into the agent's
    /// `StepCommand.retry_policy`.
    pub fn retry_or_default(&self) -> RetryPolicy {
        self.retry.clone().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_value_defaults_when_absent_or_legacy() {
        // None → all-default, and retry falls back to the engine default.
        let cfg = PipeConfig::from_value(&None);
        assert!(cfg.retry.is_none() && cfg.on_failure.is_none());
        assert_eq!(cfg.retry_or_default().max_retries, 3);

        // Legacy blob with only the old key parses cleanly (key ignored).
        let legacy = PipeConfig::from_value(&Some(json!({ "retry_count": 3 })));
        assert_eq!(legacy, PipeConfig::default());
    }

    #[test]
    fn round_trips_retry_and_handlers() {
        let cfg = PipeConfig {
            retry: Some(RetryPolicy {
                max_retries: 5,
                backoff_base_ms: 500,
                backoff_max_ms: 30_000,
            }),
            on_failure: Some(HandlerRef::Pipe("oncall-notify".into())),
            on_success: Some(HandlerRef::Notify {
                url: "https://ntfy.example.com/ok".into(),
                method: "POST".into(),
            }),
        };
        let value = serde_json::to_value(&cfg).unwrap();
        // Externally-tagged handler shape.
        assert_eq!(value["on_failure"], json!({ "pipe": "oncall-notify" }));
        assert_eq!(value["retry"]["max_retries"], json!(5));
        assert_eq!(PipeConfig::from_value(&Some(value)), cfg);
    }

    #[test]
    fn notify_method_defaults_to_post() {
        // method omitted → defaults to POST on parse.
        let h: HandlerRef =
            serde_json::from_value(json!({ "notify": { "url": "https://x/y" } })).unwrap();
        assert_eq!(
            h,
            HandlerRef::Notify {
                url: "https://x/y".into(),
                method: "POST".into()
            }
        );
    }

    #[test]
    fn merge_into_preserves_unrelated_keys() {
        let base = json!({ "retry_count": 9, "custom": true });
        let cfg = PipeConfig {
            retry: Some(RetryPolicy::default()),
            ..Default::default()
        };
        let merged = cfg.merge_into(Some(base));
        // Our typed keys are written …
        assert_eq!(merged["retry"]["max_retries"], json!(3));
        // … and pre-existing unrelated keys survive.
        assert_eq!(merged["retry_count"], json!(9));
        assert_eq!(merged["custom"], json!(true));
    }
}
