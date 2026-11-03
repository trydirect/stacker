//! Extractor for routes that require an authenticated agent.

use crate::helpers::JsonResponse;
use crate::models;
use actix_web::{dev::Payload, FromRequest, HttpMessage, HttpRequest};
use std::future::{ready, Ready};
use std::ops::Deref;
use std::sync::Arc;

/// The caller of an agent route, proven to be an agent.
///
/// `web::ReqData<Arc<models::Agent>>` cannot express this. The authentication
/// manager tries several methods, and a user's OAuth token authenticates
/// successfully as a *user* without ever setting the agent extension. The
/// handler then fails to extract it and actix answers
/// 500 "Missing expected request extension data" — a server error for what is
/// really a refusal, which is both misleading to the caller and noise in
/// monitoring. This extractor answers 401 instead.
///
/// Using it on a handler also makes the requirement part of the signature, so
/// a new agent route cannot forget the check.
pub struct AuthenticatedAgent(pub Arc<models::Agent>);

impl Deref for AuthenticatedAgent {
    type Target = models::Agent;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromRequest for AuthenticatedAgent {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        ready(
            req.extensions()
                .get::<Arc<models::Agent>>()
                .cloned()
                .map(AuthenticatedAgent)
                .ok_or_else(|| {
                    JsonResponse::<String>::unauthorized(
                        "This endpoint requires agent authentication (X-Agent-Id and a bearer agent token)",
                    )
                }),
        )
    }
}
