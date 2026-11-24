//! Pure backup logic for TryDirect Stacker (WS-1).
//!
//! Plan: config/PLAN-backups-syncopia.md. Contract:
//! config/shared-fixtures/api-contracts/backups-syncopia.json.
//!
//! The dump and restore legs are single shell commands executed through the
//! existing agent exec queue — no agent changes required. Credentials reach
//! the command through a per-run token in the environment (`BACKUP_TOKEN`),
//! never embedded in the command string.

use chrono::{DateTime, Datelike, Utc};

/// What a backup job dumps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DumpSource {
    Postgres,
    Mysql,
    Mongo,
    Volume,
}

impl DumpSource {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "postgres" => Ok(Self::Postgres),
            "mysql" => Ok(Self::Mysql),
            "mongo" => Ok(Self::Mongo),
            "volume" => Ok(Self::Volume),
            other => Err(format!("unsupported backup source: {}", other)),
        }
    }
}

/// Everything needed to build one dump/restore command.
#[derive(Debug, Clone)]
pub struct BackupSpec {
    pub source: DumpSource,
    /// Container name for database sources; volume name for `Volume`.
    pub target: String,
    /// Database name (postgres/mysql); empty uses the engine default
    /// (`$POSTGRES_DB`).
    pub database: String,
    /// Backup storage upload endpoint (POST target and file download base).
    pub endpoint: String,
    /// Extra HTTP headers for upload/download, as (name, value) pairs. Names
    /// come from configuration per destination — nothing vendor-specific is
    /// hardcoded here. Values may reference environment variables
    /// (`$BACKUP_DEVICE`, `$BACKUP_CHECKSUM`, ...).
    pub extra_headers: Vec<(String, String)>,
}

/// Shell-quote a value for a single-quoted shell string.
fn shq(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Upload pipeline shared by all sources: stream stdin to the backup storage
/// upload endpoint. The per-run bearer token comes from `$BACKUP_TOKEN`.
fn upload_pipeline(endpoint: &str, extra_headers: &[(String, String)]) -> String {
    let mut command = format!(
        "curl -fsS -X POST --data-binary @- {url} -H 'Authorization: Bearer $BACKUP_TOKEN'",
        url = shq(&format!("{}/upload", endpoint.trim_end_matches('/')))
    );
    for (name, value) in extra_headers {
        command.push_str(&format!(" -H {}", shq(&format!("{}: {}", name, value))));
    }
    command
}

/// Embed a pipeline inside `sh -c '...'`: re-escape single quotes.
fn inline(pipeline: &str) -> String {
    pipeline.replace('\'', r"'\''")
}

/// Build the dump+upload command for the agent exec queue.
pub fn build_dump_command(spec: &BackupSpec) -> Result<String, String> {
    if spec.target.trim().is_empty() {
        return Err("backup target must not be empty".to_string());
    }
    let upload = upload_pipeline(&spec.endpoint, &spec.extra_headers);
    Ok(match spec.source {
        DumpSource::Postgres => {
            let db = if spec.database.is_empty() {
                "\"$POSTGRES_DB\"".to_string()
            } else {
                shq(&spec.database)
            };
            format!(
                "docker exec {target} sh -c {script}",
                target = shq(&spec.target),
                script = shq(&inline(&format!("pg_dump -Fc {} | {}", db, upload)))
            )
        }
        DumpSource::Mysql => format!(
            "docker exec {target} sh -c {script}",
            target = shq(&spec.target),
            script = shq(&inline(
                &format!(
                    "mysqldump -uroot -p\"$MYSQL_ROOT_PASSWORD\" --all-databases | {}",
                    upload
                )
            ))
        ),
        DumpSource::Mongo => format!(
            "docker exec {target} sh -c {script}",
            target = shq(&spec.target),
            script = shq(&inline(&format!("mongodump --archive --quiet | {}", upload)))
        ),
        DumpSource::Volume => format!(
            "docker run --rm -v {mount} alpine sh -c {script}",
            mount = shq(&format!("{}:/data:ro", spec.target)),
            script = shq(&inline(&format!("tar -C /data -cf - . | gzip | {}", upload)))
        ),
    })
}

/// Build the download+restore command for the agent exec queue.
pub fn build_restore_command(spec: &BackupSpec, remote_file_id: &str) -> Result<String, String> {
    if spec.target.trim().is_empty() {
        return Err("backup target must not be empty".to_string());
    }
    if remote_file_id.trim().is_empty() {
        return Err("remote file id must not be empty".to_string());
    }
    let download = format!(
        "curl -fsS {url} -H 'Authorization: Bearer $BACKUP_TOKEN'",
        url = shq(&format!(
            "{}/upload/files/{}",
            spec.endpoint.trim_end_matches('/'),
            remote_file_id
        ))
    );
    Ok(match spec.source {
        DumpSource::Postgres => {
            let db = if spec.database.is_empty() {
                "\"$POSTGRES_DB\"".to_string()
            } else {
                shq(&spec.database)
            };
            format!(
                "{download} | docker exec -i {target} sh -c {script}",
                target = shq(&spec.target),
                script = shq(&inline(&format!(
                    "pg_restore --clean --if-exists -d {}",
                    db
                )))
            )
        }
        DumpSource::Mysql => format!(
            "{download} | docker exec -i {target} sh -c {script}",
            target = shq(&spec.target),
            script = shq(r#"mysql -uroot -p"$MYSQL_ROOT_PASSWORD""#)
        ),
        DumpSource::Mongo => format!(
            "{download} | docker exec -i {target} sh -c {script}",
            target = shq(&spec.target),
            script = shq("mongorestore --archive --quiet --drop")
        ),
        DumpSource::Volume => format!(
            "{download} | docker run --rm -i -v {mount} alpine sh -c {script}",
            mount = shq(&format!("{}:/data", spec.target)),
            script = shq("gunzip | tar -C /data -xf -")
        ),
    })
}

/// Retention decision (B2): keep the newest `daily` runs of distinct days
/// plus the newest run of each of the newest `weekly` distinct ISO weeks.
/// Everything else is pruned. Newest-first; ties break by run id.
pub fn runs_to_prune<T: RetentionRun>(runs: &[T], daily: usize, weekly: usize) -> Vec<T::Id> {
    let mut sorted: Vec<&T> = runs.iter().collect();
    sorted.sort_by(|a, b| {
        b.finished_at()
            .cmp(&a.finished_at())
            .then(b.key().cmp(&a.key()))
    });

    let mut keep: Vec<bool> = vec![false; sorted.len()];
    let mut days_kept = 0usize;
    let mut weeks_kept = 0usize;
    let mut last_day: Option<(i32, u32)> = None;
    let mut last_week: Option<(i32, u32)> = None;

    for (index, run) in sorted.iter().enumerate() {
        let at = run.finished_at();
        let day = (at.year(), at.ordinal());
        let week = (at.iso_week().year(), at.iso_week().week());

        if last_day != Some(day) && days_kept < daily {
            keep[index] = true;
            days_kept += 1;
            last_day = Some(day);
        }
        if last_week != Some(week) && weeks_kept < weekly {
            keep[index] = true;
            weeks_kept += 1;
            last_week = Some(week);
        }
    }

    sorted
        .iter()
        .zip(keep)
        .filter(|(_, keep)| !keep)
        .map(|(run, _)| run.id())
        .collect()
}

/// Minimal surface `runs_to_prune` needs from a backup run.
pub trait RetentionRun {
    type Id: Copy;
    fn id(&self) -> Self::Id;
    fn finished_at(&self) -> DateTime<Utc>;
    /// Tie-breaker (newest first); the run id is fine.
    fn key(&self) -> i64;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[derive(Clone)]
    struct Run {
        id: i32,
        at: DateTime<Utc>,
    }

    impl RetentionRun for Run {
        type Id = i32;
        fn id(&self) -> i32 {
            self.id
        }
        fn finished_at(&self) -> DateTime<Utc> {
            self.at
        }
        fn key(&self) -> i64 {
            i64::from(self.id)
        }
    }

    fn daily_runs(count: i64) -> Vec<Run> {
        let start = Utc.with_ymd_and_hms(2026, 11, 1, 3, 0, 0).unwrap();
        (0..count)
            .map(|i| Run {
                id: i as i32,
                at: start + Duration::days(i),
            })
            .collect()
    }

    #[test]
    fn test_dump_command_postgres_embeds_target_and_endpoint() {
        let spec = BackupSpec {
            source: DumpSource::Postgres,
            target: "shop-postgres-1".to_string(),
            database: "shop".to_string(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: vec![
                (
                    "X-Backup-Device-Id".to_string(),
                    "$BACKUP_DEVICE".to_string(),
                ),
                (
                    "X-Backup-Checksum".to_string(),
                    "$BACKUP_CHECKSUM".to_string(),
                ),
            ],
        };
        let command = build_dump_command(&spec).unwrap();
        assert!(command.contains("docker exec 'shop-postgres-1'"));
        assert!(command.contains("pg_dump -Fc"));
        assert!(command.contains("shop"));
        assert!(command.contains("https://backup.example/upload"));
        assert!(command.contains("$BACKUP_TOKEN"));
        // Configured headers render generically, whatever they are named.
        assert!(command.contains("X-Backup-Device-Id: $BACKUP_DEVICE"));
        assert!(command.contains("X-Backup-Checksum: $BACKUP_CHECKSUM"));
    }

    #[test]
    fn test_upload_without_extra_headers_has_only_auth() {
        let spec = BackupSpec {
            source: DumpSource::Mongo,
            target: "shop-mongo-1".to_string(),
            database: String::new(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: Vec::new(),
        };
        let command = build_dump_command(&spec).unwrap();
        assert!(command.contains("Authorization: Bearer $BACKUP_TOKEN"));
        assert_eq!(command.matches("-H '").count(), 1);
    }

    #[test]
    fn test_dump_command_volume_uses_tar_gzip() {
        let spec = BackupSpec {
            source: DumpSource::Volume,
            target: "postgres_data".to_string(),
            database: String::new(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: Vec::new(),
        };
        let command = build_dump_command(&spec).unwrap();
        assert!(command.contains("-v 'postgres_data:/data:ro'"));
        assert!(command.contains("tar -C /data"));
    }

    #[test]
    fn test_dump_command_rejects_empty_target() {
        let spec = BackupSpec {
            source: DumpSource::Postgres,
            target: "  ".to_string(),
            database: String::new(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: Vec::new(),
        };
        assert!(build_dump_command(&spec).is_err());
    }

    #[test]
    fn test_restore_command_targets_file_and_service() {
        let spec = BackupSpec {
            source: DumpSource::Postgres,
            target: "shop-postgres-1".to_string(),
            database: "shop".to_string(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: Vec::new(),
        };
        let command = build_restore_command(&spec, "f-1").unwrap();
        assert!(command.contains("https://backup.example/upload/files/f-1"));
        assert!(command.contains("pg_restore --clean --if-exists"));
        assert!(command.contains("docker exec -i 'shop-postgres-1'"));
    }

    #[test]
    fn test_restore_command_rejects_empty_file_id() {
        let spec = BackupSpec {
            source: DumpSource::Mysql,
            target: "shop-mysql-1".to_string(),
            database: String::new(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: Vec::new(),
        };
        assert!(build_restore_command(&spec, " ").is_err());
    }

    #[test]
    fn test_retention_keeps_recent_days() {
        // Fewer runs than the daily budget: nothing is pruned.
        let runs = daily_runs(5);
        assert!(runs_to_prune(&runs, 7, 4).is_empty());
    }

    #[test]
    fn test_retention_prunes_old_runs() {
        let runs = daily_runs(30);
        let pruned = runs_to_prune(&runs, 7, 4);

        // The newest 7 daily runs always stay.
        for id in 23..30 {
            assert!(!pruned.contains(&id), "run {} must stay", id);
        }
        // The oldest runs are gone.
        for id in 0..3 {
            assert!(pruned.contains(&id), "run {} must be pruned", id);
        }
        // Weekly anchors add at most 4 more kept runs beyond the 7 daily.
        let kept = 30 - pruned.len();
        assert!((7..=11).contains(&kept), "kept {} runs", kept);
    }

    #[test]
    fn test_retention_zero_budgets_prunes_everything() {
        let runs = daily_runs(3);
        assert_eq!(runs_to_prune(&runs, 0, 0).len(), 3);
    }

    #[test]
    fn test_retention_empty() {
        assert!(runs_to_prune::<Run>(&[], 7, 4).is_empty());
    }
}
