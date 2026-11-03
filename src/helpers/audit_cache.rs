//! Result caching for `/api/audit/*`: identical inputs return a cached response
//! for a short TTL, so repeated "tweak-and-resubmit" requests (and especially
//! the expensive `image` checker's Docker Hub / Trivy work) aren't redone.
//!
//! Keyed by `sha256(body)` per checker. The key derivation is pure/tested; the
//! get/set helpers wrap Redis and are best-effort (errors are swallowed so a
//! cache outage never breaks a request).

use redis::aio::ConnectionManager;
use sha2::{Digest, Sha256};

/// `audit:cache:{checker}:{sha256(body)}`.
pub fn cache_key(checker: &str, body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(body);
    format!("audit:cache:{}:{:x}", checker, hasher.finalize())
}

/// Best-effort read of a cached response body (JSON string).
pub async fn get(redis: &mut ConnectionManager, key: &str) -> Option<String> {
    let result: redis::RedisResult<Option<String>> =
        redis::cmd("GET").arg(key).query_async(redis).await;
    result.ok().flatten()
}

/// Best-effort write with a TTL (no-op on error or when `ttl_secs == 0`).
pub async fn set(redis: &mut ConnectionManager, key: &str, value: &str, ttl_secs: u64) {
    if ttl_secs == 0 {
        return;
    }
    let result: redis::RedisResult<()> = redis::cmd("SET")
        .arg(key)
        .arg(value)
        .arg("EX")
        .arg(ttl_secs)
        .query_async(redis)
        .await;
    let _ = result;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_and_input_sensitive() {
        let a = cache_key("compose", b"services: {}");
        let b = cache_key("compose", b"services: {}");
        let c = cache_key("compose", b"services: { web: {} }");
        let d = cache_key("dockerfile", b"services: {}");
        assert_eq!(a, b, "same checker+body -> same key");
        assert_ne!(a, c, "different body -> different key");
        assert_ne!(a, d, "different checker -> different key");
        assert!(a.starts_with("audit:cache:compose:"));
    }
}
