//! Encryption for chat-session message blobs.
//!
//! Messages are stored encrypted at rest using the same AES-256-GCM primitive
//! as cloud credentials ([`crate::helpers::cloud::security::Secret`], keyed by
//! the `SECURITY_KEY` env var). The stored form is
//! `base64(nonce || ciphertext)`; an empty string represents an empty session.

use crate::helpers::cloud::security::Secret;
use serde_json::Value;

/// Encrypt a JSON messages blob for storage.
///
/// An empty/absent array is stored as an empty string (no ciphertext) so the
/// "empty session" case never depends on `SECURITY_KEY` being present.
pub fn encrypt_messages(messages: &Value) -> Result<String, String> {
    if is_empty_messages(messages) {
        return Ok(String::new());
    }
    let plaintext = serde_json::to_string(messages)
        .map_err(|e| format!("Failed to serialize messages: {e}"))?;
    let secret = Secret::new();
    let ciphertext = secret.encrypt(plaintext)?;
    Ok(Secret::b64_encode(&ciphertext))
}

/// Decrypt a stored blob back into a JSON messages array.
///
/// An empty stored string decrypts to an empty array.
pub fn decrypt_messages(stored: &str) -> Result<Value, String> {
    if stored.is_empty() {
        return Ok(Value::Array(vec![]));
    }
    let bytes = Secret::b64_decode(&stored.to_string())?;
    let mut secret = Secret::new();
    let plaintext = secret.decrypt(bytes)?;
    serde_json::from_str(&plaintext).map_err(|e| format!("Failed to parse messages: {e}"))
}

fn is_empty_messages(messages: &Value) -> bool {
    match messages {
        Value::Null => true,
        Value::Array(a) => a.is_empty(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::helpers::cloud::security::security_key_test_lock as env_lock;
    use serde_json::json;

    const TEST_KEY: &str = "01234567890123456789012345678901";

    #[test]
    fn empty_messages_store_as_empty_string_without_key() {
        let _lock = env_lock::lock();
        std::env::remove_var("SECURITY_KEY");
        assert_eq!(encrypt_messages(&json!([])).unwrap(), "");
        assert_eq!(encrypt_messages(&Value::Null).unwrap(), "");
    }

    #[test]
    fn empty_stored_decrypts_to_empty_array() {
        assert_eq!(decrypt_messages("").unwrap(), json!([]));
    }

    #[test]
    fn roundtrip_encrypts_and_hides_plaintext() {
        let _lock = env_lock::lock();
        std::env::set_var("SECURITY_KEY", TEST_KEY);
        let messages = json!([
            {"role": "user", "content": "TOP-SECRET-PROMPT"},
            {"role": "assistant", "content": "hi"}
        ]);
        let stored = encrypt_messages(&messages).unwrap();
        assert!(!stored.is_empty());
        assert!(
            !stored.contains("TOP-SECRET-PROMPT"),
            "ciphertext must not contain plaintext"
        );
        let decrypted = decrypt_messages(&stored).unwrap();
        assert_eq!(decrypted, messages);
        std::env::remove_var("SECURITY_KEY");
    }
}
