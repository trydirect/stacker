use actix_casbin_auth::{
    casbin::{function_map::key_match2, CoreApi, DefaultModel},
    CasbinService,
};
use sqlx::postgres::{PgPool, PgPoolOptions};
use sqlx_adapter::SqlxAdapter;
use std::io::{Error, ErrorKind};
use tokio::time::{interval, Duration};
use tracing::{debug, warn};

pub async fn try_new(db_connection_address: String) -> Result<CasbinService, Error> {
    let m = DefaultModel::from_file("access_control.conf")
        .await
        .map_err(|err| Error::new(ErrorKind::Other, format!("{err:?}")))?;
    let a = SqlxAdapter::new(db_connection_address, 8)
        .await
        .map_err(|err| Error::new(ErrorKind::Other, format!("{err:?}")))?;

    let policy_pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&db_connection_address)
        .await
        .map_err(|err| Error::new(ErrorKind::Other, format!("{err:?}")))?;

    let casbin_service = CasbinService::new(m, a)
        .await
        .map_err(|err| Error::new(ErrorKind::Other, format!("{err:?}")))?;

    casbin_service
        .write()
        .await
        .get_role_manager()
        .write()
        .matching_fn(Some(key_match2), None);

    start_policy_reloader(casbin_service.clone(), policy_pool);

    Ok(casbin_service)
}

fn start_policy_reloader(casbin_service: CasbinService, policy_pool: PgPool) {
    // Reload Casbin policies only when the underlying rules change.
    actix_web::rt::spawn(async move {
        let mut ticker = interval(Duration::from_secs(10));
        let mut last_fingerprint: Option<(i64, i64)> = None;
        loop {
            ticker.tick().await;
            match fetch_policy_fingerprint(&policy_pool).await {
                Ok(fingerprint) => {
                    if last_fingerprint.map_or(true, |prev| prev != fingerprint) {
                        if let Err(err) = casbin_service.write().await.load_policy().await {
                            warn!("Failed to reload Casbin policies: {err:?}");
                        } else {
                            debug!("Casbin policies reloaded");
                            last_fingerprint = Some(fingerprint);
                        }
                    }
                }
                Err(err) => warn!("Failed to check Casbin policies: {err:?}"),
            }
        }
    });
}

async fn fetch_policy_fingerprint(pool: &PgPool) -> Result<(i64, i64), sqlx::Error> {
    let max_id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id), 0) FROM casbin_rule")
        .fetch_one(pool)
        .await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM casbin_rule")
        .fetch_one(pool)
        .await?;
    Ok((max_id, count))
}
