//! Pure rate-limit logic for the public `/api/audit/*` endpoints — key
//! derivation, tier selection and the allow/deny decision. Kept free of actix
//! and redis so it is unit-testable in isolation; the middleware
//! (`crate::middleware::rate_limit`) supplies the IP, clock and Redis counters.

/// Configurable limits (fixed 60s windows). Defaults are conservative.
#[derive(Debug, Clone)]
pub struct AuditRateLimitConfig {
    /// Requests/min/IP for the cheap (parse-only) checkers.
    pub per_min: u32,
    /// Requests/min/IP for the expensive `image` checker (network + Trivy).
    pub image_per_min: u32,
    /// Global requests/min ceiling across all IPs (host protection).
    pub global_per_min: u32,
    /// Max request body size in KiB.
    pub max_body_kb: usize,
    /// TTL for cached identical-input results (seconds). 0 disables caching.
    pub cache_ttl_secs: u64,
}

impl Default for AuditRateLimitConfig {
    fn default() -> Self {
        AuditRateLimitConfig {
            per_min: 30,
            image_per_min: 5,
            global_per_min: 600,
            max_body_kb: 256,
            cache_ttl_secs: 60,
        }
    }
}

impl AuditRateLimitConfig {
    /// Overlay defaults with `AUDIT_*` environment variables.
    pub fn from_env() -> Self {
        let d = Self::default();
        let num = |key: &str, dflt: u32| {
            std::env::var(key)
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(dflt)
        };
        AuditRateLimitConfig {
            per_min: num("AUDIT_RATE_LIMIT_PER_MIN", d.per_min),
            image_per_min: num("AUDIT_IMAGE_RATE_LIMIT_PER_MIN", d.image_per_min),
            global_per_min: num("AUDIT_GLOBAL_PER_MIN", d.global_per_min),
            max_body_kb: std::env::var("AUDIT_MAX_BODY_KB")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.max_body_kb),
            cache_ttl_secs: std::env::var("AUDIT_CACHE_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(d.cache_ttl_secs),
        }
    }
}

/// Cost tier for a given audit path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Cheap,
    Image,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Cheap => "cheap",
            Tier::Image => "image",
        }
    }
}

/// The `image` checker is the only expensive tier; everything else is cheap.
pub fn tier_for_path(path: &str) -> Tier {
    if path.trim_end_matches('/').ends_with("/image") {
        Tier::Image
    } else {
        Tier::Cheap
    }
}

pub fn limit_for(cfg: &AuditRateLimitConfig, tier: Tier) -> u32 {
    match tier {
        Tier::Cheap => cfg.per_min,
        Tier::Image => cfg.image_per_min,
    }
}

/// Fixed-window bucket for the current time (seconds since epoch).
pub fn window_of(now_secs: u64) -> u64 {
    now_secs / 60
}

/// Seconds until the current window resets — used for `Retry-After`.
pub fn retry_after_secs(now_secs: u64) -> u64 {
    60 - (now_secs % 60)
}

pub fn ip_key(ip: &str, tier: Tier, window: u64) -> String {
    format!("audit:rl:{}:{}:{}", tier.as_str(), ip, window)
}

pub fn global_key(window: u64) -> String {
    format!("audit:rl:global:{}", window)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Per-IP limit hit -> 429.
    Throttled {
        retry_after: u64,
    },
    /// Global ceiling hit -> 503.
    Overloaded {
        retry_after: u64,
    },
}

/// Decide from the post-INCR counter values. `count_after_incr` is the IP
/// counter after incrementing; `global_after_incr` the global counter.
pub fn decide(
    count_after_incr: u64,
    limit: u32,
    global_after_incr: u64,
    global_limit: u32,
    retry_after: u64,
) -> Decision {
    if global_after_incr > global_limit as u64 {
        return Decision::Overloaded { retry_after };
    }
    if count_after_incr > limit as u64 {
        return Decision::Throttled { retry_after };
    }
    Decision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_path_is_the_only_expensive_tier() {
        assert_eq!(tier_for_path("/api/audit/image"), Tier::Image);
        assert_eq!(tier_for_path("/api/audit/image/"), Tier::Image);
        assert_eq!(tier_for_path("/api/audit/compose"), Tier::Cheap);
        assert_eq!(tier_for_path("/api/audit/cost"), Tier::Cheap);
    }

    #[test]
    fn limits_follow_tier() {
        let cfg = AuditRateLimitConfig::default();
        assert_eq!(limit_for(&cfg, Tier::Cheap), 30);
        assert_eq!(limit_for(&cfg, Tier::Image), 5);
    }

    #[test]
    fn window_and_retry_after() {
        assert_eq!(window_of(125), 2);
        assert_eq!(retry_after_secs(125), 55); // 60 - (125 % 60) = 60 - 5
        assert_eq!(retry_after_secs(120), 60);
    }

    #[test]
    fn keys_are_namespaced_and_windowed() {
        assert_eq!(
            ip_key("1.2.3.4", Tier::Image, 7),
            "audit:rl:image:1.2.3.4:7"
        );
        assert_eq!(global_key(7), "audit:rl:global:7");
    }

    #[test]
    fn allows_up_to_the_limit() {
        // 30th request in the cheap window is still allowed.
        assert_eq!(decide(30, 30, 100, 600, 42), Decision::Allow);
        // 31st is throttled.
        assert_eq!(
            decide(31, 30, 100, 600, 42),
            Decision::Throttled { retry_after: 42 }
        );
    }

    #[test]
    fn global_ceiling_takes_precedence() {
        // Even a first-for-this-IP request is rejected when the box is overloaded.
        assert_eq!(
            decide(1, 30, 601, 600, 12),
            Decision::Overloaded { retry_after: 12 }
        );
    }
}
