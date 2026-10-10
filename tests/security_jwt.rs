mod common;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::json;

/// A bearer token that looks like a JWT is taken as the admin service's
/// login. Its role decides what Casbin allows, so the token must be signed
/// by the admin service: a token anyone can write must not be accepted.

fn forged_jwt(role: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(json!({ "alg": "HS256", "typ": "JWT" }).to_string());
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "role": role,
            "email": "forged@example.com",
            "exp": chrono::Utc::now().timestamp() + 3600
        })
        .to_string(),
    );
    format!("{header}.{payload}.forged-signature")
}

#[tokio::test]
async fn a_jwt_with_a_forged_signature_is_not_accepted() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };

    for path in ["/admin/project/user/test_user_id", "/api/admin/templates"] {
        let resp = reqwest::Client::new()
            .get(format!("{}{}", app.address, path))
            .header(
                "Authorization",
                format!("Bearer {}", forged_jwt("group_admin")),
            )
            .send()
            .await
            .expect("request failed");
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        assert!(
            status == 401 || status == 403,
            "forged admin JWT got {status} on {path}: {}",
            body.chars().take(300).collect::<String>()
        );
    }
}
