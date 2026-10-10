use crate::connectors::{
    extract_bearer_token, parse_jwt_claims, user_from_jwt_claims, validate_jwt_expiration,
    verify_jwt_signature, ADMIN_JWT_SECRET_ENV,
};
use crate::middleware::authentication::get_header;
use actix_web::dev::ServiceRequest;
use actix_web::HttpMessage;
use std::sync::Arc;

#[tracing::instrument(name = "Authenticate with JWT (admin service)")]
pub async fn try_jwt(req: &mut ServiceRequest) -> Result<bool, String> {
    let authorization = get_header::<String>(req, "authorization")?;
    if authorization.is_none() {
        return Ok(false);
    }

    let authorization = authorization.unwrap();

    // Extract Bearer token from header
    let token = match extract_bearer_token(&authorization) {
        Ok(t) => t,
        Err(_) => {
            return Ok(false); // Not a Bearer token, try other auth methods
        }
    };

    // Parse JWT claims (validates structure and expiration)
    let claims = match parse_jwt_claims(token) {
        Ok(c) => c,
        Err(err) => {
            tracing::debug!("JWT parsing failed: {}", err);
            return Ok(false); // Not a valid JWT, try other auth methods
        }
    };

    // The role in the claims becomes the Casbin subject: accept it only from
    // a token the admin service signed. Without a configured key no JWT is
    // accepted (fail closed); the request falls through to the other methods.
    let secret = std::env::var(ADMIN_JWT_SECRET_ENV).unwrap_or_default();
    if let Err(err) = verify_jwt_signature(token, &secret) {
        tracing::warn!("JWT rejected: {}", err);
        return Ok(false);
    }

    // Validate token hasn't expired
    if let Err(err) = validate_jwt_expiration(&claims) {
        tracing::warn!("JWT validation failed: {}", err);
        return Err(err);
    }

    // Create User from JWT claims
    let user = user_from_jwt_claims(&claims);

    // control access using user role
    tracing::debug!("ACL check for JWT role: {}", user.role);
    let acl_vals = actix_casbin_auth::CasbinVals {
        subject: user.role.clone(),
        domain: None,
    };

    if req.extensions_mut().insert(Arc::new(user)).is_some() {
        return Err("user already logged".to_string());
    }

    if req.extensions_mut().insert(acl_vals).is_some() {
        return Err("Something wrong with access control".to_string());
    }

    tracing::info!("JWT authentication successful for role: {}", claims.role);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::try_jwt;
    use actix_web::test::TestRequest;
    use actix_web::HttpMessage;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use serde_json::json;
    use std::sync::Arc;

    const TEST_SECRET: &str = "unit-test-admin-jwt-secret";

    fn sign(header: &str, payload: &str, secret: &str) -> String {
        use hmac::{Hmac, Mac};
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{header}.{payload}").as_bytes());
        URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
    }

    fn make_jwt_signed_with(role: &str, email: &str, exp: i64, secret: &str) -> String {
        std::env::set_var(crate::connectors::ADMIN_JWT_SECRET_ENV, TEST_SECRET);
        let header = URL_SAFE_NO_PAD.encode(json!({"alg":"HS256","typ":"JWT"}).to_string());
        let payload =
            URL_SAFE_NO_PAD.encode(json!({"role": role, "email": email, "exp": exp}).to_string());
        let signature = sign(&header, &payload, secret);
        format!("{}.{}.{}", header, payload, signature)
    }

    fn make_jwt(role: &str, email: &str, exp: i64) -> String {
        make_jwt_signed_with(role, email, exp, TEST_SECRET)
    }

    #[actix_web::test]
    async fn no_authorization_header_skips_jwt() {
        let mut req = TestRequest::default().to_srv_request();
        let result = try_jwt(&mut req).await;
        assert_eq!(result, Ok(false));
    }

    #[actix_web::test]
    async fn non_bearer_scheme_skips_jwt() {
        let mut req = TestRequest::default()
            .insert_header(("authorization", "Basic dXNlcjpwYXNz"))
            .to_srv_request();
        let result = try_jwt(&mut req).await;
        assert_eq!(result, Ok(false));
    }

    #[actix_web::test]
    async fn malformed_jwt_not_three_parts_skips() {
        let mut req = TestRequest::default()
            .insert_header(("authorization", "Bearer notajwt"))
            .to_srv_request();
        let result = try_jwt(&mut req).await;
        assert_eq!(result, Ok(false));
    }

    #[actix_web::test]
    async fn expired_jwt_returns_error() {
        let past_exp = chrono::Utc::now().timestamp() - 3600;
        let token = make_jwt("admin_service", "x@x.com", past_exp);
        let header_value = format!("Bearer {}", token);
        let mut req = TestRequest::default()
            .insert_header(("authorization", header_value.as_str()))
            .to_srv_request();
        let result = try_jwt(&mut req).await;
        assert!(
            result.is_err(),
            "expected Err for expired JWT, got {:?}",
            result
        );
    }

    #[actix_web::test]
    async fn valid_jwt_sets_user_in_extensions() {
        let future_exp = chrono::Utc::now().timestamp() + 3600;
        let token = make_jwt("admin_service", "admin@test.com", future_exp);
        let header_value = format!("Bearer {}", token);
        let mut req = TestRequest::default()
            .insert_header(("authorization", header_value.as_str()))
            .to_srv_request();
        let result = try_jwt(&mut req).await;
        assert_eq!(result, Ok(true));
        let user_present = req.extensions().get::<Arc<crate::models::User>>().is_some();
        assert!(user_present, "expected User to be inserted into extensions");
    }

    #[actix_web::test]
    async fn jwt_with_a_forged_signature_is_not_accepted() {
        let future_exp = chrono::Utc::now().timestamp() + 3600;
        for token in [
            make_jwt_signed_with("group_admin", "x@x.com", future_exp, "not-the-key"),
            {
                let t = make_jwt("group_admin", "x@x.com", future_exp);
                let (unsigned, _) = t.rsplit_once('.').unwrap();
                format!("{unsigned}.fakesig")
            },
        ] {
            let header_value = format!("Bearer {}", token);
            let mut req = TestRequest::default()
                .insert_header(("authorization", header_value.as_str()))
                .to_srv_request();
            assert_eq!(try_jwt(&mut req).await, Ok(false));
            assert!(req.extensions().get::<Arc<crate::models::User>>().is_none());
        }
    }

    #[actix_web::test]
    async fn jwt_with_alg_none_is_not_accepted() {
        let future_exp = chrono::Utc::now().timestamp() + 3600;
        std::env::set_var(crate::connectors::ADMIN_JWT_SECRET_ENV, TEST_SECRET);
        let header = URL_SAFE_NO_PAD.encode(json!({"alg":"none","typ":"JWT"}).to_string());
        let payload = URL_SAFE_NO_PAD.encode(
            json!({"role": "group_admin", "email": "x@x.com", "exp": future_exp}).to_string(),
        );
        let header_value = format!("Bearer {header}.{payload}.");
        let mut req = TestRequest::default()
            .insert_header(("authorization", header_value.as_str()))
            .to_srv_request();
        assert_eq!(try_jwt(&mut req).await, Ok(false));
    }
}
