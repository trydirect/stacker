//! Redis-backed rate limiter for the public `/api/audit/*` scope.
//!
//! Per-IP fixed-window limits (a stricter tier for `image`) plus a global
//! ceiling, shared across replicas via Redis. Fail-open: if Redis is
//! unreachable the request is allowed (the body-size cap and, when Redis
//! recovers, the counters still protect the service). Decision logic lives in
//! `crate::helpers::rate_limit` and is unit-tested there.

use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::{SystemTime, UNIX_EPOCH};

use actix_web::body::EitherBody;
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform};
use actix_web::{HttpResponse, HttpResponseBuilder};
use futures_util::future::{ok, LocalBoxFuture, Ready};
use redis::aio::ConnectionManager;

use crate::helpers::rate_limit::{
    decide, global_key, ip_key, limit_for, retry_after_secs, tier_for_path, window_of,
    AuditRateLimitConfig, Decision,
};

#[derive(Clone)]
pub struct AuditRateLimit {
    redis: ConnectionManager,
    cfg: AuditRateLimitConfig,
}

impl AuditRateLimit {
    pub fn new(redis: ConnectionManager, cfg: AuditRateLimitConfig) -> Self {
        AuditRateLimit { redis, cfg }
    }
}

impl<S, B> Transform<S, ServiceRequest> for AuditRateLimit
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Transform = AuditRateLimitMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(AuditRateLimitMiddleware {
            service: Rc::new(service),
            redis: self.redis.clone(),
            cfg: self.cfg.clone(),
        })
    }
}

pub struct AuditRateLimitMiddleware<S> {
    service: Rc<S>,
    redis: ConnectionManager,
    cfg: AuditRateLimitConfig,
}

impl<S, B> Service<ServiceRequest> for AuditRateLimitMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = self.service.clone();
        let mut redis = self.redis.clone();
        let cfg = self.cfg.clone();

        let path = req.path().to_string();
        let ip = req
            .connection_info()
            .realip_remote_addr()
            .unwrap_or("unknown")
            .to_string();

        Box::pin(async move {
            let now = now_secs();
            let window = window_of(now);
            let retry = retry_after_secs(now);
            let tier = tier_for_path(&path);
            let limit = limit_for(&cfg, tier);

            let decision =
                evaluate(&mut redis, &ip, tier, window, limit, cfg.global_per_min, retry).await;

            match decision {
                Decision::Allow => Ok(service.call(req).await?.map_into_left_body()),
                Decision::Throttled { retry_after } => {
                    Ok(reject(req, HttpResponse::TooManyRequests(), retry_after, "rate limit exceeded"))
                }
                Decision::Overloaded { retry_after } => Ok(reject(
                    req,
                    HttpResponse::ServiceUnavailable(),
                    retry_after,
                    "service busy, try again shortly",
                )),
            }
        })
    }
}

/// INCR the global then per-IP counters (fail-open on any Redis error).
async fn evaluate(
    redis: &mut ConnectionManager,
    ip: &str,
    tier: crate::helpers::rate_limit::Tier,
    window: u64,
    limit: u32,
    global_limit: u32,
    retry: u64,
) -> Decision {
    let global = match incr_with_ttl(redis, &global_key(window)).await {
        Ok(c) => c,
        Err(_) => return Decision::Allow, // fail-open
    };
    let per_ip = match incr_with_ttl(redis, &ip_key(ip, tier, window)).await {
        Ok(c) => c,
        Err(_) => return Decision::Allow,
    };
    decide(per_ip, limit, global, global_limit, retry)
}

/// `INCR key`; set a 60s TTL on first use so the window expires.
async fn incr_with_ttl(redis: &mut ConnectionManager, key: &str) -> redis::RedisResult<u64> {
    let count: u64 = redis::cmd("INCR").arg(key).query_async(redis).await?;
    if count == 1 {
        let _: () = redis::cmd("EXPIRE").arg(key).arg(60).query_async(redis).await?;
    }
    Ok(count)
}

fn reject<B>(
    req: ServiceRequest,
    mut builder: HttpResponseBuilder,
    retry_after: u64,
    message: &str,
) -> ServiceResponse<EitherBody<B>> {
    let resp = builder
        .insert_header(("Retry-After", retry_after.to_string()))
        .json(serde_json::json!({ "error": message, "retry_after": retry_after }));
    req.into_response(resp).map_into_right_body()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
