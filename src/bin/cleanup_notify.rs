use chrono::Utc;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions, PgSslMode};
use sqlx::Row;
use stacker::configuration::get_configuration;
use stacker::helpers::MqManager;
use stacker::services::project_cleanup_notifier;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = stacker::telemetry::get_subscriber("cleanup-notify".into(), "info".into());
    stacker::telemetry::init_subscriber(subscriber);

    let settings = get_configuration().expect("Failed to read configuration.");

    let connect_options = PgConnectOptions::new()
        .host(&settings.database.host)
        .port(settings.database.port)
        .username(&settings.database.username)
        .password(&settings.database.password)
        .database(&settings.database.database_name)
        .ssl_mode(PgSslMode::Disable);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await
        .expect("Failed to connect to database.");

    let mq_manager = MqManager::try_new(settings.amqp.connection_string())
        .expect("Failed to connect to RabbitMQ.");

    let http_client = reqwest::Client::new();

    let user_service_url = settings
        .connectors
        .user_service
        .as_ref()
        .map(|c| c.base_url.clone())
        .unwrap_or_else(|| "http://localhost:4100/server/user".to_string());

    let internal_key =
        std::env::var("INTERNAL_SERVICES_ACCESS_KEY").unwrap_or_default();

    // Find projects that are marked for deletion but haven't been notified yet
    let rows = sqlx::query(
        r#"
        SELECT id, name, user_id, deletion_scheduled_at
        FROM project
        WHERE deletion_scheduled_at IS NOT NULL
          AND deletion_warning_sent_at IS NULL
        ORDER BY id
        "#,
    )
    .fetch_all(&pool)
    .await?;

    if rows.is_empty() {
        tracing::info!("No new projects to notify about.");
        return Ok(());
    }

    tracing::info!("Found {} projects needing deletion warnings.", rows.len());

    let mut sent = 0;
    let mut failed = 0;

    for row in &rows {
        let project_id: i32 = row.get("id");
        let project_name: String = row.get("name");
        let user_id: String = row.get("user_id");
        let deletion_scheduled_at: chrono::DateTime<Utc> = row.get("deletion_scheduled_at");

        let result = project_cleanup_notifier::notify_project_deletion_warning(
            &mq_manager,
            &http_client,
            &user_service_url,
            &internal_key,
            project_id,
            &project_name,
            &user_id,
            deletion_scheduled_at,
        )
        .await;

        if result.email_sent || result.bell_sent {
            // Mark as notified
            sqlx::query(
                "UPDATE project SET deletion_warning_sent_at = NOW() at time zone 'utc' WHERE id = $1",
            )
            .bind(project_id)
            .execute(&pool)
            .await?;

            sent += 1;
            tracing::info!(
                "Notified user {} about project {} ({})",
                user_id,
                project_name,
                project_id
            );
        } else {
            failed += 1;
            tracing::warn!(
                "Failed to notify user {} about project {} ({}): {:?}",
                user_id,
                project_name,
                project_id,
                result.error
            );
        }
    }

    tracing::info!(
        "Cleanup notification complete: {} sent, {} failed, {} total",
        sent,
        failed,
        rows.len()
    );

    Ok(())
}
