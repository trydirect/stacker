use actix_casbin_auth::{
    CasbinService,
    casbin::{
        DefaultModel,
        CoreApi,
        function_map::key_match2
    }
};
use std::io::{Error, ErrorKind};
use sqlx_adapter::SqlxAdapter;

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

    Ok(casbin_service)
}
