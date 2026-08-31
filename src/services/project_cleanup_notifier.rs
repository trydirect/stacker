use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::helpers::MqManager;

/// Message payload for the notify service email channel.
#[derive(Debug, Serialize)]
struct NotifyEmailMessage {
    to: String,
    subject: String,
    message: String,
    html: Option<String>,
}

/// Result of notifying a user about a project scheduled for deletion.
#[derive(Debug)]
pub struct NotificationResult {
    pub project_id: i32,
    pub project_name: String,
    pub user_id: String,
    pub email_sent: bool,
    pub bell_sent: bool,
    pub error: Option<String>,
}

/// Resolve user email from the User Service by user_id.
async fn resolve_user_email(
    http_client: &reqwest::Client,
    user_service_url: &str,
    internal_key: &str,
    user_id: &str,
) -> Result<String, String> {
    let url = format!(
        "{}/users/{}",
        user_service_url.trim_end_matches('/'),
        user_id
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

    user.get("email")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("No email found for user {}", user_id))
}

/// Send a deletion warning email via RabbitMQ to the notify service.
pub async fn send_deletion_warning_email(
    mq_manager: &MqManager,
    user_email: &str,
    project_name: &str,
    project_id: i32,
    deletion_date: DateTime<Utc>,
) -> Result<(), String> {
    let days_remaining = (deletion_date - Utc::now()).num_days().max(1);
    let date_str = deletion_date.format("%B %d, %Y").to_string();

    let msg = NotifyEmailMessage {
        to: user_email.to_string(),
        subject: format!(
            "Your project '{}' will be deleted in {} days",
            project_name, days_remaining
        ),
        message: format!(
            "Your project '{}' has been inactive and is scheduled for automatic deletion on {}. \
             To keep this project, open it in the stack builder and make any change (even just saving). \
             If no action is taken, the project and all its data will be permanently removed.",
            project_name, date_str
        ),
        html: Some(format!(
            r#"<div style="font-family:Arial,sans-serif;max-width:600px;margin:0 auto">
                <h2 style="color:#e65100">Project Deletion Warning</h2>
                <p>Your project <strong>{}</strong> has been inactive and is scheduled for automatic deletion on <strong>{}</strong>.</p>
                <p>To keep this project, open it in the stack builder and make any change (even just saving).</p>
                <p style="color:#666;font-size:14px">If no action is taken, the project and all its data will be permanently removed.</p>
                <a href="https://try.direct/applications/stack-builder?project={}" style="display:inline-block;padding:12px 24px;background:#1976d2;color:#fff;text-decoration:none;border-radius:4px;margin-top:16px">Open Project</a>
            </div>"#,
            project_name, date_str, project_id
        )),
    };

    mq_manager
        .publish(
            "notify".to_string(),
            "notify.start.email.project_deletion_warning.all".to_string(),
            &msg,
        )
        .await?;

    Ok(())
}

/// Create a bell notification via the User Service HTTP API.
pub async fn create_bell_notification(
    http_client: &reqwest::Client,
    user_service_url: &str,
    internal_key: &str,
    user_id: &str,
    project_name: &str,
    deletion_date: DateTime<Utc>,
) -> Result<(), String> {
    let days_remaining = (deletion_date - Utc::now()).num_days().max(1);
    let date_str = deletion_date.format("%B %d, %Y").to_string();

    let url = format!("{}/notifications/", user_service_url.trim_end_matches('/'));

    let response = http_client
        .post(&url)
        .header("X-Internal-Service-Key", internal_key)
        .json(&serde_json::json!({
            "user_id": user_id,
            "event_type": "project_deletion_warning",
            "title": format!("Project '{}' scheduled for deletion", project_name),
            "message": format!(
                "This project will be deleted in {} days (on {}) due to inactivity. \
                 Open it in the stack builder and save to keep it.",
                days_remaining, date_str
            ),
            "event_metadata": {
                "project_name": project_name,
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

/// Notify a user about a project scheduled for deletion (both email + bell).
pub async fn notify_project_deletion_warning(
    mq_manager: &MqManager,
    http_client: &reqwest::Client,
    user_service_url: &str,
    internal_key: &str,
    project_id: i32,
    project_name: &str,
    user_id: &str,
    deletion_date: DateTime<Utc>,
) -> NotificationResult {
    let mut result = NotificationResult {
        project_id,
        project_name: project_name.to_string(),
        user_id: user_id.to_string(),
        email_sent: false,
        bell_sent: false,
        error: None,
    };

    // Resolve user email from User Service
    let user_email =
        match resolve_user_email(http_client, user_service_url, internal_key, user_id).await {
            Ok(email) => email,
            Err(e) => {
                tracing::warn!("Failed to resolve email for user {}: {}", user_id, e);
                result.error = Some(format!("email_resolve: {}", e));
                // Still try bell notification even if email fails
                match create_bell_notification(
                    http_client,
                    user_service_url,
                    internal_key,
                    user_id,
                    project_name,
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

    match send_deletion_warning_email(
        mq_manager,
        &user_email,
        project_name,
        project_id,
        deletion_date,
    )
    .await
    {
        Ok(()) => {
            result.email_sent = true;
            tracing::info!(
                "Sent deletion warning email for project {} to {}",
                project_id,
                user_email
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to send deletion warning email for project {}: {}",
                project_id,
                e
            );
            result.error = Some(format!("email: {}", e));
        }
    }

    match create_bell_notification(
        http_client,
        user_service_url,
        internal_key,
        user_id,
        project_name,
        deletion_date,
    )
    .await
    {
        Ok(()) => {
            result.bell_sent = true;
            tracing::info!(
                "Created bell notification for project {} for user {}",
                project_id,
                user_id
            );
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create bell notification for project {}: {}",
                project_id,
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
