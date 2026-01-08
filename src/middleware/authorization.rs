use actix_casbin_auth::{
    casbin::{function_map::key_match2, CoreApi, DefaultModel},
    CasbinService,
};
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

    let casbin_service = CasbinService::new(m, a)
        .await
        .map_err(|err| Error::new(ErrorKind::Other, format!("{err:?}")))?;

    casbin_service
        .write()
        .await
        .get_role_manager()
        .write()
        .matching_fn(Some(key_match2), None);

    start_policy_reloader(casbin_service.clone());

    Ok(casbin_service)
}

fn start_policy_reloader(casbin_service: CasbinService) {
    // Periodically reload Casbin policies so new Casbin migrations apply without restarts.
    actix_web::rt::spawn(async move {
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            if let Err(err) = casbin_service.write().await.load_policy().await {
                warn!("Failed to reload Casbin policies: {err:?}");
            } else {
                debug!("Casbin policies reloaded");
            }
        }
    });
}
