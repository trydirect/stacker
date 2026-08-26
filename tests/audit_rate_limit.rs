//! Integration test for the /api/audit/* rate limiter.
//!
//! Redis-gated: if no Redis is reachable at `REDIS_URL` (default
//! redis://127.0.0.1/0) the test prints a skip notice and returns — mirroring
//! the Postgres-gated BDD harness. When Redis is present it drives a real actix
//! app and asserts the Nth+1 request in a window is throttled with 429.

use actix_web::{test, App};
use stacker::helpers::rate_limit::AuditRateLimitConfig;
use stacker::routes;

async fn connect_redis() -> Option<redis::aio::ConnectionManager> {
    let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/0".to_string());
    let client = redis::Client::open(url.as_str()).ok()?;
    // Bound the connect so the test skips fast when Redis is absent (the
    // ConnectionManager otherwise retries for minutes).
    match tokio::time::timeout(
        std::time::Duration::from_secs(2),
        redis::aio::ConnectionManager::new(client),
    )
    .await
    {
        Ok(Ok(cm)) => Some(cm),
        _ => None,
    }
}

#[actix_web::test]
async fn compose_endpoint_throttles_after_limit() {
    let Some(conn) = connect_redis().await else {
        eprintln!("skipping audit rate-limit test: no Redis at REDIS_URL");
        return;
    };

    let cfg = AuditRateLimitConfig {
        per_min: 3,
        image_per_min: 3,
        global_per_min: 1_000_000, // effectively disabled for this test
        max_body_kb: 256,
        cache_ttl_secs: 0, // disable caching so every request reaches the limiter
    };

    let app = test::init_service(
        App::new().configure(move |c| routes::audit::configure(c, Some(conn.clone()), cfg.clone())),
    )
    .await;

    // Unique client IP (via X-Forwarded-For) so this run's counters don't
    // collide with other runs sharing the same 60s window.
    let ip = format!("203.0.113.{}", (std::process::id() % 250) + 1);

    let mut statuses = Vec::new();
    for _ in 0..4 {
        let req = test::TestRequest::post()
            .uri("/api/audit/compose")
            .insert_header(("X-Forwarded-For", ip.clone()))
            .insert_header(("content-type", "text/plain"))
            .set_payload("services: {}")
            .to_request();
        let resp = test::call_service(&app, req).await;
        statuses.push(resp.status().as_u16());
    }

    // per_min = 3 -> first three allowed, fourth throttled.
    assert_eq!(&statuses[..3], &[200, 200, 200], "statuses: {statuses:?}");
    assert_eq!(
        statuses[3], 429,
        "expected 429 on the 4th request, got {statuses:?}"
    );
}
