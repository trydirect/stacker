//! Shared gate for endpoints called by other TryDirect services rather than by
//! users or agents.
//!
//! These endpoints are granted to `group_anonymous` in Casbin because the
//! caller presents no user credential and would otherwise be denied before the
//! handler runs. Casbin therefore decides only *reachability*; this module is
//! the actual authorisation, and every such endpoint must call it.
//!
//! Keep the two layers in mind when adding one: a Casbin rule alone admits the
//! request, it does not authorise it.

use actix_web::{error::ErrorUnauthorized, HttpRequest, Result};
use subtle::ConstantTimeEq;

/// Header carrying the shared service key.
pub const INTERNAL_KEY_HEADER: &str = "x-internal-key";

/// Environment variable holding its expected value.
pub const INTERNAL_KEY_ENV: &str = "INTERNAL_SERVICES_ACCESS_KEY";

/// Authorise a service-to-service call, or fail.
///
/// **Fails closed when the variable is unset**: a deployment that has not
/// configured the key rejects these calls rather than accepting them. Note
/// that the usual `unwrap_or_default()` reflex would do the opposite here, so
/// the empty case is checked before the comparison.
pub fn require_internal_key(req: &HttpRequest) -> Result<()> {
    let expected = std::env::var(INTERNAL_KEY_ENV).unwrap_or_default();
    if expected.is_empty() {
        tracing::error!(
            "{} is not configured; rejecting internal service call",
            INTERNAL_KEY_ENV
        );
        return Err(ErrorUnauthorized("invalid internal key"));
    }

    let provided = req
        .headers()
        .get(INTERNAL_KEY_HEADER)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    // Constant-time: this compares secrets directly, unlike the agent token
    // path which compares digests.
    let matches: bool = provided.as_bytes().ct_eq(expected.as_bytes()).into();

    if !matches {
        return Err(ErrorUnauthorized("invalid internal key"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    /// Serialises the tests, which share one process-wide environment.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_key<T>(value: Option<&str>, body: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var(INTERNAL_KEY_ENV).ok();
        match value {
            Some(v) => std::env::set_var(INTERNAL_KEY_ENV, v),
            None => std::env::remove_var(INTERNAL_KEY_ENV),
        }
        let out = body();
        match previous {
            Some(v) => std::env::set_var(INTERNAL_KEY_ENV, v),
            None => std::env::remove_var(INTERNAL_KEY_ENV),
        }
        out
    }

    #[test]
    fn accepts_the_configured_key() {
        with_key(Some("s3cret"), || {
            let req = TestRequest::default()
                .insert_header((INTERNAL_KEY_HEADER, "s3cret"))
                .to_http_request();
            assert!(require_internal_key(&req).is_ok());
        });
    }

    #[test]
    fn rejects_a_wrong_or_missing_header() {
        with_key(Some("s3cret"), || {
            for header in [None, Some(""), Some("s3crea"), Some("s3cret ")] {
                let mut req = TestRequest::default();
                if let Some(value) = header {
                    req = req.insert_header((INTERNAL_KEY_HEADER, value));
                }
                assert!(
                    require_internal_key(&req.to_http_request()).is_err(),
                    "header {header:?} must be rejected"
                );
            }
        });
    }

    /// The case worth pinning: an unconfigured deployment must refuse these
    /// calls. An empty expected value compared with `unwrap_or_default()` would
    /// otherwise match a request that sends no header at all.
    #[test]
    fn rejects_everything_when_the_key_is_not_configured() {
        with_key(None, || {
            let bare = TestRequest::default().to_http_request();
            assert!(require_internal_key(&bare).is_err());

            let empty_header = TestRequest::default()
                .insert_header((INTERNAL_KEY_HEADER, ""))
                .to_http_request();
            assert!(require_internal_key(&empty_header).is_err());
        });

        with_key(Some(""), || {
            let bare = TestRequest::default().to_http_request();
            assert!(require_internal_key(&bare).is_err());
        });
    }
}
