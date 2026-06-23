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

/// Recursively walks a JSON value and replaces the value of any key whose name
/// matches a sensitive pattern (password, secret, api_key, …) with the string
/// `"***REDACTED***"`. Null values are left untouched. Structure (key names,
/// array order) is preserved so callers can still inspect what fields exist.
pub fn redact_sensitive_json_values(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
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

#[cfg(test)]
mod tests {
    use super::redact_sensitive_json_values;
    use serde_json::json;

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
}
