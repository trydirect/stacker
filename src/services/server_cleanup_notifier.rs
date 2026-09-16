use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::helpers::MqManager;

#[derive(Debug, Serialize)]
struct NotifyEmailMessage {
    to: String,
    subject: String,
    message: String,
    html: Option<String>,
}

#[derive(Debug)]
pub struct ServerNotificationResult {
    pub server_id: i32,
    pub server_name: String,
    pub user_id: String,
    pub email_sent: bool,
    pub bell_sent: bool,
    pub error: Option<String>,
}

async fn resolve_user_email(
    http_client: &reqwest::Client,
    user_service_url: &str,
    internal_key: &str,
    user_id: &str,
) -> Result<String, String> {
    let url = format!(
        "{}/api/internal/users/{}",
        user_service_url.trim_end_matches('/'),
        urlencoding::encode(user_id)
    );

    let response = http_client
        .get(&url)
        .header("X-Internal-Service-Key", internal_key)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch user {}: {}", user_id, e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("User Service error ({}): {}", status, body));
    }

    let user: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse user response: {}", e))?;

    if user.get("email_confirmed") == Some(&serde_json::Value::Bool(false)) {
        return Err(format!("Email for user {} is unconfirmed", user_id));
    }

    user.get("email")
        .and_then(|v| v.as_str())
        .filter(|email| !email.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No email found for user {}", user_id))
}

pub async fn send_server_deletion_warning_email(
    mq_manager: &MqManager,
    user_email: &str,
    server_name: &str,
    _server_id: i32,
    server_ip: Option<&str>,
    deletion_date: DateTime<Utc>,
) -> Result<(), String> {
    let days_remaining = (deletion_date - Utc::now()).num_days().max(1);
    let date_str = deletion_date.format("%B %d, %Y").to_string();
    let ip_display = server_ip.unwrap_or("unknown");

    let msg = NotifyEmailMessage {
        to: user_email.to_string(),
        subject: format!(
            "Your server '{}' ({}) will be deleted in {} days",
            server_name, ip_display, days_remaining
        ),
        message: format!(
            "Your server '{}' ({}) has been inactive and is scheduled for automatic deletion on {}. \
             To keep this server, open it in the stack builder and make any change (even just saving). \
             If no action is taken, the server and all its data will be permanently removed.",
            server_name, ip_display, date_str
        ),
        html: Some(format!(
            r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:0 auto">
                <h2 style="color:#e65100">Server Deletion Warning</h2>
                <p>Your server <strong>{}</strong> (<code>{}</code>) has been inactive and is scheduled for automatic deletion on <strong>{}</strong>.</p>
                <p>To keep this server, open it in the stack builder and make any change (even just saving).</p>
                <p style="color:#666;font-size:14px">If no action is taken, the server and all its data will be permanently removed.</p>
                <a href="https://try.direct/applications/stack-builder" style="display:inline-block;padding:12px 24px;background:#1976d2;color:#fff;text-decoration:none;border-radius:4px;margin-top:16px">Open Stack Builder</a>
            </div>"#,
            server_name, ip_display, date_str
        )),
    };

    mq_manager
        .publish(
            "notify".to_string(),
            "notify.start.email.server_deletion_warning.all".to_string(),
            &msg,
        )
        .await?;

    Ok(())
}

pub async fn create_server_bell_notification(
    http_client: &reqwest::Client,
    user_service_url: &str,
    internal_key: &str,
    user_id: &str,
    server_name: &str,
    server_ip: Option<&str>,
    deletion_date: DateTime<Utc>,
) -> Result<(), String> {
    let days_remaining = (deletion_date - Utc::now()).num_days().max(1);
    let date_str = deletion_date.format("%B %d, %Y").to_string();
    let ip_display = server_ip.unwrap_or("unknown");

    let url = format!("{}/notifications/", user_service_url.trim_end_matches('/'));

    let response = http_client
        .post(&url)
        .header("X-Internal-Service-Key", internal_key)
        .json(&serde_json::json!({
            "user_id": user_id,
            "event_type": "server_deletion_warning",
            "title": format!("Server '{}' ({}) scheduled for deletion", server_name, ip_display),
            "message": format!(
                "This server will be deleted in {} days (on {}) due to inactivity. \
                 Open it in the stack builder and save to keep it.",
                days_remaining, date_str
            ),
            "event_metadata": {
                "server_name": server_name,
                "server_ip": ip_display,
                "deletion_date": deletion_date.to_rfc3339(),
                "days_remaining": days_remaining,
            }
        }))
        .send()
        .await
        .map_err(|e| format!("Failed to create bell notification: {}", e))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!(
            "User Service notification error ({}): {}",
            status, body
        ));
    }

    Ok(())
}

pub async fn notify_server_deletion_warning(
    mq_manager: &MqManager,
    http_client: &reqwest::Client,
    user_service_url: &str,
    internal_key: &str,
    server_id: i32,
    server_name: &str,
    server_ip: Option<&str>,
    user_id: &str,
    deletion_date: DateTime<Utc>,
) -> ServerNotificationResult {
    let mut result = ServerNotificationResult {
        server_id,
        server_name: server_name.to_string(),
        user_id: user_id.to_string(),
        email_sent: false,
        bell_sent: false,
        error: None,
    };

    let user_email =
        match resolve_user_email(http_client, user_service_url, internal_key, user_id).await {
            Ok(email) => email,
            Err(e) => {
                tracing::warn!("Failed to resolve email for user {}: {}", user_id, e);
                result.error = Some(format!("email_resolve: {}", e));
                match create_server_bell_notification(
                    http_client,
                    user_service_url,
                    internal_key,
                    user_id,
                    server_name,
                    server_ip,
                    deletion_date,
                )
                .await
                {
                    Ok(()) => result.bell_sent = true,
                    Err(e) => {
                        result.error = Some(format!(
                            "email_resolve: {}, bell: {}",
                            result.error.unwrap_or_default(),
                            e
                        ))
                    }
                }
                return result;
            }
        };

    match send_server_deletion_warning_email(
        mq_manager,
        &user_email,
        server_name,
        server_id,
        server_ip,
        deletion_date,
    )
    .await
    {
        Ok(()) => {
            result.email_sent = true;
            tracing::info!(
                "Sent server deletion warning email for server {} to {}",
                server_id,
                user_email
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to send server deletion warning email for server {}: {}",
                server_id,
                e
            );
            result.error = Some(format!("email: {}", e));
        }
    }

    match create_server_bell_notification(
        http_client,
        user_service_url,
        internal_key,
        user_id,
        server_name,
        server_ip,
        deletion_date,
    )
    .await
    {
        Ok(()) => {
            result.bell_sent = true;
            tracing::info!(
                "Created bell notification for server {} for user {}",
                server_id,
                user_id
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create bell notification for server {}: {}",
                server_id,
                e
            );
            result.error = Some(format!(
                "{}bell: {}",
                result.error.map(|s| s + ", ").unwrap_or_default(),
                e
            ));
        }
    }

    result
}
