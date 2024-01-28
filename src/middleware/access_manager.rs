use actix_casbin_auth::casbin::{DefaultModel, FileAdapter};//, Result};
use actix_casbin_auth::CasbinService;
use actix_casbin_auth::casbin::function_map::key_match2;
use actix_casbin_auth::casbin::CoreApi;
use std::io::{Error, ErrorKind};

pub async fn try_new() -> Result<CasbinService, Error> {
    let m = DefaultModel::from_file("rbac/rbac_with_pattern_model.conf")
        .await
        .map_err(|err| Error::new(ErrorKind::Other, format!("{err:?}")))?;
    let a = FileAdapter::new("rbac/rbac_with_pattern_policy.csv");  

    let mut casbin_service = CasbinService::new(m, a)
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
