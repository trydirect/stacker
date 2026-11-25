//! Pure backup logic for TryDirect Stacker (WS-1).
//!
//! Plan: config/PLAN-backups.md. Contract:
//! config/shared-fixtures/api-contracts/backups-storage.json.
//!
//! The dump and restore legs run on the target server through the agent
//! command queue. This crate turns a backup spec into a typed execution plan
//! (process argv + upload/download metadata); the agent runs the process and
//! streams its stdout/stdin to the storage backend natively. No shell command
//! strings, no HTTP client here. The per-run token reaches the agent through
//! the environment (`BACKUP_TOKEN`), never embedded in argv.
//!
//! The storage backend is pluggable: endpoints and upload header names come
//! from destination configuration, nothing vendor-specific is hardcoded.

use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};

/// Where the dump comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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

/// Everything needed to build one dump/restore plan.
#[derive(Debug, Clone)]
pub struct BackupSpec {
    pub source: DumpSource,
    /// Container name for database sources; volume name for `Volume`.
    pub target: String,
    /// Database name (postgres/mysql); empty uses the engine default from the
    /// container environment.
    pub database: String,
    /// Backup storage upload endpoint (upload base and file download base).
    pub endpoint: String,
    /// Extra HTTP headers for upload/download, as (name, value) pairs. Names
    /// come from configuration per destination — nothing vendor-specific is
    /// hardcoded here. Values may reference environment variables that the
    /// agent resolves at run time (`BACKUP_DEVICE`, `BACKUP_CHECKSUM`, ...).
    pub extra_headers: Vec<(String, String)>,
}

/// Where the produced archive goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadTarget {
    /// `POST {endpoint}/upload`.
    pub url: String,
    /// Bearer token env var name; the agent reads the token from there.
    pub auth_token_env: String,
    /// Configured extra headers (device id, checksum, ...).
    pub headers: Vec<(String, String)>,
}

/// Where the restore source archive comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTarget {
    /// `GET {endpoint}/upload/files/{file_id}`.
    pub url: String,
    /// Bearer token env var name.
    pub auth_token_env: String,
    pub headers: Vec<(String, String)>,
}

/// Typed execution plan for a dump: run `argv`, stream stdout to `upload`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DumpPlan {
    pub argv: Vec<String>,
    pub upload: UploadTarget,
}

/// Typed execution plan for a restore: stream `download` into stdin of `argv`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub argv: Vec<String>,
    pub download: DownloadTarget,
}

/// Environment variable holding the per-run bearer token.
pub const TOKEN_ENV: &str = "BACKUP_TOKEN";

fn validate_target(spec: &BackupSpec) -> Result<(), String> {
    if spec.target.trim().is_empty() {
        return Err("backup target must not be empty".to_string());
    }
    if spec.endpoint.trim().is_empty() {
        return Err("backup endpoint must not be empty".to_string());
    }
    Ok(())
}

fn upload_target(spec: &BackupSpec) -> UploadTarget {
    UploadTarget {
        url: format!("{}/upload", spec.endpoint.trim_end_matches('/')),
        auth_token_env: TOKEN_ENV.to_string(),
        headers: spec.extra_headers.clone(),
    }
}

fn download_target(spec: &BackupSpec, remote_file_id: &str) -> DownloadTarget {
    DownloadTarget {
        url: format!(
            "{}/upload/files/{}",
            spec.endpoint.trim_end_matches('/'),
            remote_file_id
        ),
        auth_token_env: TOKEN_ENV.to_string(),
        headers: spec.extra_headers.clone(),
    }
}

/// In-container script form: when the engine default database or a password
/// from the container environment is needed, the command runs through
/// `sh -c` inside the target container. The script is passed as a single argv
/// element — there is no host shell, so no escaping is involved. With
/// `interactive`, stdin is attached (restore leg).
fn container_script(target: &str, script: &str, interactive: bool) -> Vec<String> {
    let mut argv = vec!["docker".to_string(), "exec".to_string()];
    if interactive {
        argv.push("-i".to_string());
    }
    argv.push(target.to_string());
    argv.extend(["sh".to_string(), "-c".to_string(), script.to_string()]);
    argv
}

/// Build the dump plan: run `argv`, stream its stdout to the upload target.
pub fn build_dump_plan(spec: &BackupSpec) -> Result<DumpPlan, String> {
    validate_target(spec)?;
    let upload = upload_target(spec);

    let argv = match spec.source {
        DumpSource::Postgres => {
            let db_arg = if spec.database.is_empty() {
                "\"$POSTGRES_DB\"".to_string()
            } else {
                format!("'{}'", spec.database.replace('\'', r"'\''"))
            };
            if spec.database.is_empty() {
                container_script(&spec.target, &format!("pg_dump -Fc {}", db_arg), false)
            } else {
                vec![
                    "docker".to_string(),
                    "exec".to_string(),
                    spec.target.clone(),
                    "pg_dump".to_string(),
                    "-Fc".to_string(),
                    spec.database.clone(),
                ]
            }
        }
        DumpSource::Mysql => container_script(
            &spec.target,
            r#"mysqldump -uroot -p"$MYSQL_ROOT_PASSWORD" --all-databases"#,
            false,
        ),
        DumpSource::Mongo => vec![
            "docker".to_string(),
            "exec".to_string(),
            spec.target.clone(),
            "mongodump".to_string(),
            "--archive".to_string(),
            "--quiet".to_string(),
        ],
        DumpSource::Volume => vec![
            "docker".to_string(),
            "run".to_string(),
            "--rm".to_string(),
            "-v".to_string(),
            format!("{}:/data:ro", spec.target),
            "alpine".to_string(),
            "tar".to_string(),
            "-C".to_string(),
            "/data".to_string(),
            "-cf".to_string(),
            "-".to_string(),
            ".".to_string(),
        ],
    };

    Ok(DumpPlan { argv, upload })
}

/// Build the restore plan: stream the download into stdin of `argv`.
pub fn build_restore_plan(spec: &BackupSpec, remote_file_id: &str) -> Result<RestorePlan, String> {
    validate_target(spec)?;
    if remote_file_id.trim().is_empty() {
        return Err("remote file id must not be empty".to_string());
    }
    let download = download_target(spec, remote_file_id);

    let argv = match spec.source {
        DumpSource::Postgres => {
            if spec.database.is_empty() {
                container_script(
                    &spec.target,
                    r#"pg_restore --clean --if-exists -d "$POSTGRES_DB""#,
                    true,
                )
            } else {
                vec![
                    "docker".to_string(),
                    "exec".to_string(),
                    "-i".to_string(),
                    spec.target.clone(),
                    "pg_restore".to_string(),
                    "--clean".to_string(),
                    "--if-exists".to_string(),
                    "-d".to_string(),
                    spec.database.clone(),
                ]
            }
        }
        DumpSource::Mysql => container_script(
            &spec.target,
            r#"mysql -uroot -p"$MYSQL_ROOT_PASSWORD""#,
            true,
        ),
        DumpSource::Mongo => vec![
            "docker".to_string(),
            "exec".to_string(),
            "-i".to_string(),
            spec.target.clone(),
            "mongorestore".to_string(),
            "--archive".to_string(),
            "--quiet".to_string(),
            "--drop".to_string(),
        ],
        DumpSource::Volume => vec![
            "docker".to_string(),
            "run".to_string(),
            "--rm".to_string(),
            "-i".to_string(),
            "-v".to_string(),
            format!("{}:/data", spec.target),
            "alpine".to_string(),
            "tar".to_string(),
            "-C".to_string(),
            "/data".to_string(),
            "-xf".to_string(),
            "-".to_string(),
        ],
    };

    Ok(RestorePlan { argv, download })
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

    fn spec(source: DumpSource, target: &str, database: &str) -> BackupSpec {
        BackupSpec {
            source,
            target: target.to_string(),
            database: database.to_string(),
            endpoint: "https://backup.example".to_string(),
            extra_headers: Vec::new(),
        }
    }

    #[test]
    fn test_dump_plan_postgres_named_database_is_pure_argv() {
        let spec = spec(DumpSource::Postgres, "shop-postgres-1", "shop");
        let plan = build_dump_plan(&spec).unwrap();
        assert_eq!(
            plan.argv,
            vec![
                "docker",
                "exec",
                "shop-postgres-1",
                "pg_dump",
                "-Fc",
                "shop"
            ]
        );
        assert_eq!(plan.upload.url, "https://backup.example/upload");
        assert_eq!(plan.upload.auth_token_env, "BACKUP_TOKEN");
    }

    #[test]
    fn test_dump_plan_postgres_default_database_uses_container_env() {
        let spec = spec(DumpSource::Postgres, "shop-postgres-1", "");
        let plan = build_dump_plan(&spec).unwrap();
        // argv carries the script as one element: no host shell, no escaping.
        assert_eq!(plan.argv[0], "docker");
        assert_eq!(plan.argv[3], "sh");
        assert_eq!(plan.argv[4], "-c");
        assert_eq!(plan.argv[5], r#"pg_dump -Fc "$POSTGRES_DB""#);
    }

    #[test]
    fn test_dump_plan_carries_configured_headers() {
        let mut spec = spec(DumpSource::Mongo, "shop-mongo-1", "");
        spec.extra_headers = vec![
            (
                "X-Backup-Device-Id".to_string(),
                "$BACKUP_DEVICE".to_string(),
            ),
            (
                "X-Backup-Checksum".to_string(),
                "$BACKUP_CHECKSUM".to_string(),
            ),
        ];
        let plan = build_dump_plan(&spec).unwrap();
        // Configured headers travel as data, whatever they are named.
        assert_eq!(plan.upload.headers.len(), 2);
        assert_eq!(plan.upload.headers[0].0, "X-Backup-Device-Id");
        assert_eq!(plan.upload.headers[1].1, "$BACKUP_CHECKSUM");
        // The process argv itself never contains secrets or headers.
        assert!(plan.argv.iter().all(|a| !a.contains("TOKEN")));
    }

    #[test]
    fn test_dump_plan_mongo_and_volume_argv() {
        let mongo = build_dump_plan(&spec(DumpSource::Mongo, "shop-mongo-1", "")).unwrap();
        assert_eq!(
            mongo.argv,
            vec![
                "docker",
                "exec",
                "shop-mongo-1",
                "mongodump",
                "--archive",
                "--quiet"
            ]
        );

        let volume = build_dump_plan(&spec(DumpSource::Volume, "postgres_data", "")).unwrap();
        assert_eq!(
            volume.argv,
            vec![
                "docker",
                "run",
                "--rm",
                "-v",
                "postgres_data:/data:ro",
                "alpine",
                "tar",
                "-C",
                "/data",
                "-cf",
                "-",
                "."
            ]
        );
    }

    #[test]
    fn test_dump_plan_rejects_empty_target() {
        let mut spec = spec(DumpSource::Postgres, "  ", "");
        spec.endpoint = "https://backup.example".to_string();
        assert!(build_dump_plan(&spec).is_err());
    }

    #[test]
    fn test_restore_plan_targets_file_and_service() {
        let spec = spec(DumpSource::Postgres, "shop-postgres-1", "shop");
        let plan = build_restore_plan(&spec, "f-1").unwrap();
        assert_eq!(plan.download.url, "https://backup.example/upload/files/f-1");
        assert_eq!(
            plan.argv,
            vec![
                "docker",
                "exec",
                "-i",
                "shop-postgres-1",
                "pg_restore",
                "--clean",
                "--if-exists",
                "-d",
                "shop"
            ]
        );
    }

    #[test]
    fn test_restore_plan_default_database_uses_container_env() {
        let spec = spec(DumpSource::Postgres, "shop-postgres-1", "");
        let plan = build_restore_plan(&spec, "f-1").unwrap();
        assert_eq!(
            plan.argv[6],
            r#"pg_restore --clean --if-exists -d "$POSTGRES_DB""#
        );
    }

    #[test]
    fn test_restore_plan_rejects_empty_file_id() {
        let spec = spec(DumpSource::Mysql, "shop-mysql-1", "");
        assert!(build_restore_plan(&spec, " ").is_err());
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
