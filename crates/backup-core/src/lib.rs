//! Pure backup logic for TryDirect Stacker (WS-1).
//!
//! Plan: config/PLAN-backups.md. Contract:
//! config/shared-fixtures/api-contracts/backups-storage.json.
//!
//! The dump and restore legs run on the target server through the agent
//! command queue. This crate turns a backup spec into a typed execution plan:
//! a process `argv` plus an [`UploadTarget`]/[`DownloadTarget`] that
//! *describes the storage protocol* (body encoding + auth + headers). The
//! agent interprets the plan: it runs the process and streams the archive to
//! the backend with its native HTTP client. Nothing vendor-specific is
//! hardcoded — the protocol is data, so the backend is pluggable (a REST
//! backup-storage service today, a restic or S3 target tomorrow).
//!
//! Secrets never appear in a plan: auth references environment-variable
//! *names* (e.g. `BACKUP_TOKEN`, `AGENT_ID`, client-secret env vars), and the
//! agent resolves them at run time.

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

/// How the archive is encoded into the upload request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UploadBody {
    /// The archive stream is the request body.
    Raw,
    /// `multipart/form-data`: the archive rides in `file_field`, plus literal
    /// form fields (values may reference environment variables).
    Multipart {
        file_field: String,
        fields: Vec<FormField>,
    },
}

/// A literal form field; `value` may reference an env var (`$AGENT_ID`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormField {
    pub name: String,
    pub value: String,
}

/// How a request is authenticated. Every credential is referenced by env-var
/// name — never carried as a value in the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Auth {
    /// No authentication.
    None,
    /// `Authorization: Bearer <token>`; the token is read from `token_env`.
    Bearer { token_env: String },
    /// OAuth2 client_credentials: POST `token_url` → `access_token` → Bearer.
    /// Suits service accounts (e.g. ZITADEL). Client id/secret come from env.
    ClientCredentials {
        token_url: String,
        client_id_env: String,
        client_secret_env: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        scope: Option<String>,
    },
    /// HTTP Basic; credentials come from env.
    Basic {
        username_env: String,
        password_env: String,
    },
}

/// The upload leg of a storage backend: where to POST the archive and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadTarget {
    /// Full upload URL.
    pub url: String,
    pub body: UploadBody,
    pub auth: Auth,
    /// Extra headers; values may reference env vars.
    pub headers: Vec<(String, String)>,
}

/// The download leg (restore): where to GET a stored archive and how.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadTarget {
    /// Full file URL (the caller embeds the remote file id).
    pub url: String,
    pub auth: Auth,
    pub headers: Vec<(String, String)>,
}

/// What to dump (the storage protocol is supplied separately).
#[derive(Debug, Clone)]
pub struct BackupSpec {
    pub source: DumpSource,
    /// Container name for database sources; volume name for `Volume`.
    pub target: String,
    /// Database name (postgres/mysql); empty uses the engine default from the
    /// container environment.
    pub database: String,
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

fn validate_spec(spec: &BackupSpec) -> Result<(), String> {
    if spec.target.trim().is_empty() {
        return Err("backup target must not be empty".to_string());
    }
    Ok(())
}

fn validate_headers(headers: &[(String, String)]) -> Result<(), String> {
    for (name, value) in headers {
        if name.trim().is_empty() {
            return Err("header name must not be empty".to_string());
        }
        // Reject CR/LF so a configured value cannot inject extra headers.
        if name.contains('\r')
            || name.contains('\n')
            || value.contains('\r')
            || value.contains('\n')
        {
            return Err("header must not contain CR or LF".to_string());
        }
    }
    Ok(())
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

fn dump_argv(spec: &BackupSpec) -> Result<Vec<String>, String> {
    validate_spec(spec)?;
    Ok(match spec.source {
        DumpSource::Postgres => {
            if spec.database.is_empty() {
                container_script(&spec.target, r#"pg_dump -Fc "$POSTGRES_DB""#, false)
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
    })
}

fn restore_argv(spec: &BackupSpec) -> Result<Vec<String>, String> {
    validate_spec(spec)?;
    Ok(match spec.source {
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
    })
}

/// Build the dump plan: run `argv`, stream its stdout to `upload`.
pub fn build_dump_plan(spec: &BackupSpec, upload: UploadTarget) -> Result<DumpPlan, String> {
    if upload.url.trim().is_empty() {
        return Err("upload url must not be empty".to_string());
    }
    validate_headers(&upload.headers)?;
    if let UploadBody::Multipart { fields, .. } = &upload.body {
        for field in fields {
            if field.name.trim().is_empty() {
                return Err("multipart field name must not be empty".to_string());
            }
        }
    }
    Ok(DumpPlan {
        argv: dump_argv(spec)?,
        upload,
    })
}

/// Build the restore plan: stream `download` into stdin of `argv`.
pub fn build_restore_plan(
    spec: &BackupSpec,
    download: DownloadTarget,
) -> Result<RestorePlan, String> {
    if download.url.trim().is_empty() {
        return Err("download url must not be empty".to_string());
    }
    validate_headers(&download.headers)?;
    Ok(RestorePlan {
        argv: restore_argv(spec)?,
        download,
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

    fn spec(source: DumpSource, target: &str, database: &str) -> BackupSpec {
        BackupSpec {
            source,
            target: target.to_string(),
            database: database.to_string(),
        }
    }

    fn raw_upload() -> UploadTarget {
        UploadTarget {
            url: "https://backup.example/upload".to_string(),
            body: UploadBody::Raw,
            auth: Auth::Bearer {
                token_env: "BACKUP_TOKEN".to_string(),
            },
            headers: Vec::new(),
        }
    }

    #[test]
    fn test_dump_plan_postgres_named_database_is_pure_argv() {
        let plan = build_dump_plan(
            &spec(DumpSource::Postgres, "shop-postgres-1", "shop"),
            raw_upload(),
        )
        .unwrap();
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
    }

    #[test]
    fn test_dump_plan_postgres_default_database_uses_container_env() {
        let plan = build_dump_plan(
            &spec(DumpSource::Postgres, "shop-postgres-1", ""),
            raw_upload(),
        )
        .unwrap();
        assert_eq!(plan.argv[3], "sh");
        assert_eq!(plan.argv[5], r#"pg_dump -Fc "$POSTGRES_DB""#);
    }

    #[test]
    fn test_multipart_upload_describes_syncopia_protocol() {
        // The pluggable protocol: multipart body, ZITADEL-style client_credentials
        // auth, and a device-id form field referencing the agent env.
        let upload = UploadTarget {
            url: "https://backup.example/upload".to_string(),
            body: UploadBody::Multipart {
                file_field: "file".to_string(),
                fields: vec![FormField {
                    name: "device_id".to_string(),
                    value: "$AGENT_ID".to_string(),
                }],
            },
            auth: Auth::ClientCredentials {
                token_url: "https://login.example/oauth/v2/token".to_string(),
                client_id_env: "BACKUP_CLIENT_ID".to_string(),
                client_secret_env: "BACKUP_CLIENT_SECRET".to_string(),
                scope: Some("openid".to_string()),
            },
            headers: vec![(
                "X-Backup-Filename".to_string(),
                "$BACKUP_FILENAME".to_string(),
            )],
        };
        let plan = build_dump_plan(&spec(DumpSource::Mongo, "shop-mongo-1", ""), upload).unwrap();

        // The plan is pure data: it describes the protocol, carries no secret.
        let serialized = serde_json::to_string(&plan).unwrap();
        assert!(serialized.contains("client_credentials"));
        assert!(serialized.contains("multipart"));
        assert!(serialized.contains("$AGENT_ID"));
        assert!(!serialized.contains("secret-value"));

        match &plan.upload.body {
            UploadBody::Multipart { file_field, fields } => {
                assert_eq!(file_field, "file");
                assert_eq!(fields[0].value, "$AGENT_ID");
            }
            _ => panic!("expected multipart"),
        }
    }

    #[test]
    fn test_header_with_crlf_is_rejected() {
        let mut upload = raw_upload();
        upload
            .headers
            .push(("X-Bad".to_string(), "value\r\nInjected: 1".to_string()));
        assert!(build_dump_plan(&spec(DumpSource::Postgres, "db", "x"), upload).is_err());
    }

    #[test]
    fn test_dump_plan_rejects_empty_target_and_url() {
        assert!(build_dump_plan(&spec(DumpSource::Postgres, "  ", ""), raw_upload()).is_err());
        let mut upload = raw_upload();
        upload.url = "  ".to_string();
        assert!(build_dump_plan(&spec(DumpSource::Postgres, "db", "x"), upload).is_err());
    }

    #[test]
    fn test_restore_plan_targets_file_and_service() {
        let download = DownloadTarget {
            url: "https://backup.example/upload/files/f-1".to_string(),
            auth: Auth::Bearer {
                token_env: "BACKUP_TOKEN".to_string(),
            },
            headers: Vec::new(),
        };
        let plan = build_restore_plan(
            &spec(DumpSource::Postgres, "shop-postgres-1", "shop"),
            download,
        )
        .unwrap();
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
    fn test_restore_plan_rejects_empty_download_url() {
        let download = DownloadTarget {
            url: " ".to_string(),
            auth: Auth::None,
            headers: Vec::new(),
        };
        assert!(
            build_restore_plan(&spec(DumpSource::Mysql, "shop-mysql-1", ""), download).is_err()
        );
    }

    #[test]
    fn test_retention_keeps_recent_days() {
        let runs = daily_runs(5);
        assert!(runs_to_prune(&runs, 7, 4).is_empty());
    }

    #[test]
    fn test_retention_prunes_old_runs() {
        let runs = daily_runs(30);
        let pruned = runs_to_prune(&runs, 7, 4);
        for id in 23..30 {
            assert!(!pruned.contains(&id), "run {} must stay", id);
        }
        for id in 0..3 {
            assert!(pruned.contains(&id), "run {} must be pruned", id);
        }
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
