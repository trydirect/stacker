use crate::db;
use crate::helpers::JsonResponse;
use crate::services;
use actix_web::{post, web, HttpRequest, Responder, Result};
use sqlx::PgPool;
use std::sync::Arc;

#[tracing::instrument(name = "Payout provider webhook", skip_all)]
#[post("/payouts/webhook")]
pub async fn webhook_handler(
    req: HttpRequest,
    body: web::Bytes,
    pg_pool: web::Data<PgPool>,
    payout_provider: web::Data<Arc<dyn services::PayoutProvider>>,
) -> Result<impl Responder> {
    let signature = req
        .headers()
        .get("stripe-signature")
        .and_then(|value| value.to_str().ok());

    let update = payout_provider
        .parse_webhook_update(&body, signature)
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().bad_request(err.to_string()))?;

    let Some(update) = update else {
        return Ok(JsonResponse::<serde_json::Value>::build().ok("Webhook ignored"));
    };

    if update.onboarding_completed {
        db::marketplace::complete_vendor_onboarding_by_payout_account_ref(
            pg_pool.get_ref(),
            &update.provider,
            &update.account_ref,
            &format!("{}_webhook", update.event_type),
            update.payouts_enabled,
        )
        .await
        .map_err(|err| JsonResponse::<serde_json::Value>::build().internal_server_error(err))?;
    }

    Ok(JsonResponse::<serde_json::Value>::build().ok("Webhook processed"))
}
