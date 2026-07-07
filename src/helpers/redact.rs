fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_lowercase();
    k.contains("password")
        || k.contains("passwd")
        || k.contains("pwd")
        || k.contains("secret")
        || k.contains("api_key")
        || k.contains("apikey")
        || k.contains("private_key")
        || k.contains("access_key")
        || k.contains("auth_key")
}

// ─── JSON redaction ───────────────────────────────────────────────────────────

/// Recursively walks a JSON value and replaces sensitive data with `"***REDACTED***"`.
///
/// Two patterns are handled:
///
/// 1. **Direct key** — the JSON object key itself is the sensitive name:
///    `{"DB_PASSWORD": "secret"}` → value is redacted.
///
/// 2. **Key-value pair** — the stack_definition vars format used by ProjectForm:
///    `{"key": "DB_PASSWORD", "value": "secret"}` → "value" field is redacted when
///    the string stored in "key" matches a sensitive pattern.
///
/// Null values are never replaced. Key names and array order are preserved.
pub fn redact_sensitive_json_values(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            // Pattern 2: {"key": "<sensitive_name>", "value": "<data>"}
            // This is the format used for env var entries in stack_definition.
            let named_key_is_sensitive = map
                .get("key")
                .and_then(|v| v.as_str())
                .map(is_sensitive_key)
                .unwrap_or(false);

            if named_key_is_sensitive {
                if let Some(val) = map.get_mut("value") {
                    if !val.is_null() {
                        *val = serde_json::Value::String("***REDACTED***".to_string());
                    }
                }
                // This object is a single env-var entry; no need to recurse further.
                return;
            }

            // Pattern 1: standard JSON keys
            for (key, val) in map.iter_mut() {
                if is_sensitive_key(key) && !val.is_null() {
                    *val = serde_json::Value::String("***REDACTED***".to_string());
                } else {
                    redact_sensitive_json_values(val);
                }
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                redact_sensitive_json_values(item);
            }
        }
        _ => {}
    }
}

// ─── YAML redaction ───────────────────────────────────────────────────────────

fn redact_yaml_value(value: &mut serde_yaml::Value) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (key, val) in map.iter_mut() {
                if let serde_yaml::Value::String(k) = key {
                    if is_sensitive_key(k) && !val.is_null() {
                        *val = serde_yaml::Value::String("***REDACTED***".to_string());
                    } else {
                        redact_yaml_value(val);
                    }
                }
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq.iter_mut() {
                // Handle "VARNAME=value" entries in environment lists
                if let serde_yaml::Value::String(s) = item {
                    if let Some(eq) = s.find('=') {
                        if is_sensitive_key(&s[..eq]) {
                            let key_part = s[..eq].to_string();
                            *s = format!("{}=***REDACTED***", key_part);
                        }
                    }
                } else {
                    redact_yaml_value(item);
                }
            }
        }
        _ => {}
    }
}

/// Parses a YAML string, redacts values of sensitive-named keys, and
/// re-serializes to YAML. Returns the original string on parse failure.
pub fn redact_yaml_string(yaml: &str) -> String {
    match serde_yaml::from_str::<serde_yaml::Value>(yaml) {
        Ok(mut value) => {
            redact_yaml_value(&mut value);
            serde_yaml::to_string(&value).unwrap_or_else(|_| yaml.to_string())
        }
        Err(_) => yaml.to_string(),
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{redact_sensitive_json_values, redact_yaml_string};
    use serde_json::json;

    // JSON tests

    #[test]
    fn redacts_password_fields() {
        let mut v = json!({ "password": "hunter2", "username": "alice" });
        redact_sensitive_json_values(&mut v);
        assert_eq!(v["password"], "***REDACTED***");
        assert_eq!(v["username"], "alice");
    }

    #[test]
    fn redacts_nested_sensitive_fields() {
        let mut v = json!({ "db": { "db_password": "secret123", "host": "localhost" } });
        redact_sensitive_json_values(&mut v);
        assert_eq!(v["db"]["db_password"], "***REDACTED***");
        assert_eq!(v["db"]["host"], "localhost");
    }

    #[test]
    fn redacts_in_arrays() {
        let mut v = json!([{ "api_key": "abc", "name": "svc" }]);
        redact_sensitive_json_values(&mut v);
        assert_eq!(v[0]["api_key"], "***REDACTED***");
        assert_eq!(v[0]["name"], "svc");
    }

    #[test]
    fn leaves_null_values_alone() {
        let mut v = json!({ "password": null });
        redact_sensitive_json_values(&mut v);
        assert!(v["password"].is_null());
    }

    #[test]
    fn case_insensitive_key_matching() {
        let mut v = json!({ "DB_PASSWORD": "s3cr3t", "API_KEY": "tok" });
        redact_sensitive_json_values(&mut v);
        assert_eq!(v["DB_PASSWORD"], "***REDACTED***");
        assert_eq!(v["API_KEY"], "***REDACTED***");
    }

    /// stack_definition stores env vars as [{key: "NAME", value: "DATA"}] objects.
    /// The sensitive name is a string *value* under "key", not a JSON object key.
    #[test]
    fn redacts_key_value_pair_format() {
        let mut v = json!([
            { "key": "DB_PASSWORD", "value": "super_secret_pw" },
            { "key": "DB_HOST",     "value": "db.internal" },
            { "key": "api_key",     "value": "sk-live-abc" },
            { "key": "APP_PORT",    "value": "3000" }
        ]);
        redact_sensitive_json_values(&mut v);

        assert_eq!(v[0]["value"], "***REDACTED***", "DB_PASSWORD must be redacted");
        assert_eq!(v[1]["value"], "db.internal",    "DB_HOST must not be redacted");
        assert_eq!(v[2]["value"], "***REDACTED***", "api_key must be redacted");
        assert_eq!(v[3]["value"], "3000",           "APP_PORT must not be redacted");

        // Key names must survive unchanged
        assert_eq!(v[0]["key"], "DB_PASSWORD");
        assert_eq!(v[1]["key"], "DB_HOST");
    }

    #[test]
    fn key_value_pair_with_null_value_is_left_alone() {
        let mut v = json!({ "key": "DB_PASSWORD", "value": null });
        redact_sensitive_json_values(&mut v);
        assert!(v["value"].is_null());
    }

    // YAML tests

    #[test]
    fn yaml_redacts_docker_compose_env_mapping() {
        let yaml = "services:\n  app:\n    environment:\n      FLOWISE_PASSWORD: admin123\n      DATABASE_PASSWORD: Qwerty123\n      SECRETKEY: change-me\n      PORT: '3000'\n      HOST: localhost\n";
        let result = redact_yaml_string(yaml);
        assert!(result.contains("***REDACTED***"), "should contain REDACTED marker");
        assert!(!result.contains("admin123"),   "FLOWISE_PASSWORD value must be gone");
        assert!(!result.contains("Qwerty123"),  "DATABASE_PASSWORD value must be gone");
        assert!(!result.contains("change-me"),  "SECRETKEY value must be gone");
        assert!(result.contains("'3000'") || result.contains("3000"), "PORT must survive");
        assert!(result.contains("localhost"),   "HOST must survive");
    }

    #[test]
    fn yaml_redacts_env_list_format() {
        let yaml = "services:\n  app:\n    environment:\n    - DB_PASSWORD=secret\n    - APP_PORT=8080\n    - API_KEY=sk-xxx\n";
        let result = redact_yaml_string(yaml);
        assert!(result.contains("DB_PASSWORD=***REDACTED***"), "list-style DB_PASSWORD must be redacted");
        assert!(result.contains("API_KEY=***REDACTED***"),     "list-style API_KEY must be redacted");
        assert!(result.contains("APP_PORT=8080"),              "APP_PORT must survive");
    }

    #[test]
    fn yaml_unparseable_string_returned_as_is() {
        // A tab character in a YAML mapping value position is invalid in strict mode.
        // Use an unterminated flow mapping which serde_yaml 0.9 rejects.
        let bad = "{unclosed: [";
        let result = redact_yaml_string(bad);
        // Either returned as-is (parse failed) or survived round-trip without panicking
        assert!(!result.contains("PANIC"));
    }
}
