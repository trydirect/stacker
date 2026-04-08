mod common;

use common::{USER_A_TOKEN, USER_B_TOKEN};

/// Admin endpoints (/admin/*) are protected by Casbin RBAC.
/// Mock users have role "group_user" which has no admin policies.
/// Requests should be denied with 403 Forbidden.

#[tokio::test]
async fn test_admin_list_users_rejects_non_admin() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    // group_user → Casbin denies /admin/* → 403
    let resp = client
        .get(format!("{}/admin/rating", &app.address))
        .header("Authorization", format!("Bearer {}", USER_A_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(
        resp.status().as_u16(),
        403,
        "Regular user GET /admin/rating should return 403"
    );
}

#[tokio::test]
async fn test_admin_routes_reject_unauthenticated() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let admin_endpoints = vec![
        ("GET", format!("{}/admin/rating", &app.address)),
        ("GET", format!("{}/admin/rating/1", &app.address)),
        (
            "PUT",
            format!("{}/admin/client/1/enable", &app.address),
        ),
    ];

    for (method, url) in admin_endpoints {
        let req = match method {
            "GET" => client.get(&url),
            "PUT" => client.put(&url),
            _ => unreachable!(),
        };

        // No Authorization header → anonymous → Casbin denies → 403
        let resp = req.send().await.expect("Failed to send request");

        assert_eq!(
            resp.status().as_u16(),
            403,
            "Unauthenticated {} {} should return 403",
            method,
            url
        );
    }
}

#[tokio::test]
async fn test_admin_endpoint_not_accessible_to_regular_user() {
    let Some(app) = common::spawn_app_two_users().await else {
        return;
    };
    let client = reqwest::Client::new();

    let admin_endpoints = vec![
        ("GET", format!("{}/admin/rating", &app.address)),
        ("GET", format!("{}/admin/rating/999", &app.address)),
        (
            "PUT",
            format!("{}/admin/client/999", &app.address),
        ),
        (
            "PUT",
            format!("{}/admin/client/999/enable", &app.address),
        ),
        (
            "PUT",
            format!("{}/admin/client/999/disable", &app.address),
        ),
    ];

    for (method, url) in admin_endpoints {
        for token in [USER_A_TOKEN, USER_B_TOKEN] {
            let req = match method {
                "GET" => client.get(&url),
                "PUT" => client.put(&url),
                "DELETE" => client.delete(&url),
                _ => unreachable!(),
            };

            let resp = req
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .expect("Failed to send request");

            assert_eq!(
                resp.status().as_u16(),
                403,
                "Regular user {} {} should return 403 (token={})",
                method,
                url,
                token
            );
        }
    }
}
