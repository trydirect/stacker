use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A container that has been seen running on a deployment.
///
/// This is the dashboard's membership, and the reason it exists: the list used
/// to be recomputed on every request from the newest health report, so a report
/// that was late, partial or of the wrong shape changed what the user saw.
/// Rows are added when a container is observed and retired only on an explicit
/// event — silence makes a row stale, never absent.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeploymentContainer {
    pub id: i32,
    pub deployment_hash: String,
    pub container_name: String,
    pub app_code: Option<String>,
    /// `project` or `platform`; see [`crate::project_app::classify_scope`].
    pub scope: String,
    pub image: Option<String>,
    pub state: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    /// When an aggregate report last mentioned this container. Staleness is
    /// derived from it.
    pub last_seen_at: DateTime<Utc>,
    /// Set when the container's app was removed on purpose.
    pub removed_at: Option<DateTime<Utc>>,
}

/// One container as an aggregate health report describes it.
///
/// Separate from [`DeploymentContainer`] because a report has no identity or
/// history — it is what the upsert is given, not what it stores.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedContainer {
    pub container_name: String,
    pub app_code: Option<String>,
    pub scope: String,
    pub image: Option<String>,
    pub state: Option<String>,
}

impl DeploymentContainer {
    /// Whether the deployment has gone quiet about this container.
    ///
    /// Staleness is a display concern, not a reason to forget the row: an
    /// unreported container is shown as unknown, which is not the same as
    /// stopped and certainly not the same as gone.
    pub fn is_stale(&self, now: DateTime<Utc>, threshold: chrono::Duration) -> bool {
        now - self.last_seen_at > threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn container(last_seen_at: DateTime<Utc>) -> DeploymentContainer {
        DeploymentContainer {
            id: 1,
            deployment_hash: "deployment_x".to_string(),
            container_name: "project-app-1".to_string(),
            app_code: Some("floci".to_string()),
            scope: "project".to_string(),
            image: Some("floci/floci:latest".to_string()),
            state: Some("running".to_string()),
            first_seen_at: last_seen_at,
            last_seen_at,
            removed_at: None,
        }
    }

    #[test]
    fn a_recently_reported_container_is_not_stale() {
        let now = Utc::now();
        let row = container(now - chrono::Duration::seconds(30));

        assert!(!row.is_stale(now, chrono::Duration::minutes(5)));
    }

    #[test]
    fn silence_past_the_threshold_is_stale() {
        let now = Utc::now();
        let row = container(now - chrono::Duration::minutes(30));

        assert!(row.is_stale(now, chrono::Duration::minutes(5)));
    }

    /// The boundary belongs to the container: exactly at the threshold it is
    /// still considered reported, so a report arriving at the interval does not
    /// flicker between fresh and stale.
    #[test]
    fn the_threshold_itself_is_not_yet_stale() {
        let now = Utc::now();
        let row = container(now - chrono::Duration::minutes(5));

        assert!(!row.is_stale(now, chrono::Duration::minutes(5)));
    }
}
