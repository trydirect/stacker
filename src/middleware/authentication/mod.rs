mod manager;
mod manager_middleware;

pub use manager::*;
pub use manager_middleware::*;

use crate::{
    helpers::JsonResponse,
    models,
    forms,
    configuration::Settings,
};
use actix_http::header::CONTENT_LENGTH;
use futures::{
    future::{FutureExt, LocalBoxFuture},
    lock::Mutex,
    task::{Context, Poll},
    StreamExt,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::{
    future::{ready, Ready},
    str::FromStr,
    sync::Arc,
};
use tracing::Instrument;
use actix_web::{};
use reqwest::header::{ACCEPT, CONTENT_TYPE};
use actix_web::{ 
    web::BytesMut,
    HttpMessage,
    web,
    Error,
    dev::{ServiceRequest, Service, ServiceResponse, Transform},
    error::ErrorBadRequest,
    http::header::HeaderName,
};
use sqlx::{Pool, Postgres};

fn get_header<T>(req: &ServiceRequest, header_name: &'static str) -> Result<Option<T>, String>
where
    T: FromStr,
{
    let header_value = req
        .headers()
        .get(HeaderName::from_static(header_name));

    if header_value.is_none() {
        return Ok(None);
    }

    header_value
        .unwrap()
        .to_str()
        .map_err(|_| format!("header {header_name} can't be converted to string"))? 
        .parse::<T>()
        .map_err(|_| format!("header {header_name} has wrong type"))
        .map(|v| Some(v))
}

async fn db_fetch_client(db_pool: &Pool<Postgres>, client_id: i32) -> Result<models::Client, String> { //todo
    let query_span = tracing::info_span!("Fetching the client by ID");

    sqlx::query_as!(
        models::Client,
        r#"SELECT id, user_id, secret FROM client c WHERE c.id = $1"#,
        client_id,
        )
        .fetch_one(db_pool)
        .instrument(query_span)
        .await
        .map_err(|err| {
            match err {
                sqlx::Error::RowNotFound => "the client is not found".to_string(),
                e => {
                    tracing::error!("Failed to execute fetch query: {:?}", e);
                    String::new()
                }
            }
        })
}

async fn compute_body_hash(req: &mut ServiceRequest, client_secret: &[u8]) -> Result<String, String> {
    let content_length: usize = get_header(req, CONTENT_LENGTH.as_str())?.unwrap(); 
    let mut body = BytesMut::with_capacity(content_length);
    let mut payload = req.take_payload();
    while let Some(chunk) = payload.next().await {
        body.extend_from_slice(&chunk.expect("can't unwrap the chunk"));
    }

    let mut mac =
        match Hmac::<Sha256>::new_from_slice(client_secret) {
            Ok(mac) => mac,
            Err(err) => {
                tracing::error!("error generating hmac {err:?}");
                return Err("".to_string());
            }
        };

    mac.update(body.as_ref());
    let (_, mut payload) = actix_http::h1::Payload::create(true);
    payload.unread_data(body.into());
    req.set_payload(payload.into());

    Ok(format!("{:x}", mac.finalize().into_bytes()))
}

#[tracing::instrument(name = "try authorize. stacker-id header")]
async fn try_authorize_id_hash(req: &mut ServiceRequest, client_id: i32) -> Result<(), String> {
    let header_hash = get_header::<String>(&req, "stacker-hash")?; 
    if header_hash.is_none() {
        return Err("stacker-hash header is not set".to_string());
    } //todo
    let header_hash = header_hash.unwrap();

    let db_pool = req.app_data::<web::Data<Pool<Postgres>>>().unwrap().get_ref();
    let client: models::Client = db_fetch_client(db_pool, client_id).await?;
    if client.secret.is_none() {
        return Err("client is not active".to_string());
    }

    let client_secret = client.secret.as_ref().unwrap().as_bytes();
    let body_hash = compute_body_hash(req, client_secret).await?;
    if header_hash != body_hash {
        return Err("hash is wrong".to_string());
    }

    match req.extensions_mut().insert(Arc::new(client)) {
        Some(_) => {
            tracing::error!("client middleware already called once");
            return Err("".to_string());
        }
        None => {}
    }

    let accesscontrol_vals = actix_casbin_auth::CasbinVals {
        subject: client_id.to_string(),
        domain: None,
    };
    if req.extensions_mut().insert(accesscontrol_vals).is_some() {
        return Err("sth wrong with access control".to_string());
    }

    Ok(())
}

#[tracing::instrument(name = "try authorize. Authorization header")]
async fn try_authorize_bearer(req: &mut ServiceRequest, authorization: String) -> Result<(), String> {
    let settings = req.app_data::<web::Data<Settings>>().unwrap();
    let token = "abc"; //todo
    let user = match fetch_user(settings.auth_url.as_str(), token).await {
        Ok(user) => user,
        Err(err) => {
            return Err(format!("{}", err));
        }
    }; //todo . process the err

    if req.extensions_mut().insert(Arc::new(user)).is_some() {
        return Err("user already logged".to_string());
    }

    let accesscontrol_vals = actix_casbin_auth::CasbinVals {
        subject: String::from("alice"), //todo username or anonymous
        domain: None,
    };
    if req.extensions_mut().insert(accesscontrol_vals).is_some() {
        return Err("sth wrong with access control".to_string());
    }

    Ok(())
}

async fn fetch_user(auth_url: &str, token: &str) -> Result<models::User, String> {
    let client = reqwest::Client::new();
    let resp = client
        .get(auth_url)
        .bearer_auth(token)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_err| "no resp from auth server".to_string())?;

    if !resp.status().is_success() {
        return Err("401 Unauthorized".to_string());
    }

    resp
        .json::<forms::UserForm>()
        .await
        .map_err(|_err| "can't parse the response body".to_string())?
        .try_into()
}

