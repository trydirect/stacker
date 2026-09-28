//! Migration-on-boot contract.
//!
//! Without schema `20260828121000` (`stack_template_version.config_contract`,
//! a column with no `#[sqlx(default)]`) Stacker cannot resolve a template at
//! all. Nothing used to guarantee migrations were applied before the server
//! started: `migrations/` was not copied into the production image even though
//! the `sqlx` CLI was, and no deploy path ran `sqlx migrate run`.
//!
//! These tests pin the fix: the image ships `migrations/`, and the container
//! entrypoint applies them before exec-ing whatever command it was given.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(file)
}

/// Endpoint used only to prove the entrypoint passes a URL through to `sqlx`.
/// Nothing listens on it, so there is nothing here to disclose.
const FIXTURE_DB_URL: &str = "postgres://user:pass@db:5432/test"; // pragma: allowlist secret

fn read(file: &str) -> String {
    fs::read_to_string(manifest_path(file)).unwrap_or_else(|err| panic!("read {file}: {err}"))
}

/// Drop the `#` line comments so assertions don't trip over commented-out
/// history in the Dockerfile (there is a lot of it).
fn active_lines(dockerfile: &str) -> String {
    dockerfile
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A `sqlx` stub on PATH that records how it was invoked, so the entrypoint can
/// be exercised without a database or the real CLI.
fn stub_sqlx(bin_dir: &Path, log: &Path) -> PathBuf {
    fs::create_dir_all(bin_dir).expect("stub bin dir");
    let script = format!(
        "#!/bin/sh\nprintf 'ARGS:%s\\n' \"$*\" >> '{log}'\nprintf 'URL:%s\\n' \"$DATABASE_URL\" >> '{log}'\n",
        log = log.display()
    );
    let path = bin_dir.join("sqlx");
    fs::write(&path, script).expect("write stub sqlx");
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod stub sqlx");
    path
}

/// Run the entrypoint with the given command, returning (exit code, stub log).
fn run_entrypoint(
    workdir: &Path,
    env: &[(&str, &str)],
    args: &[&str],
) -> (Option<i32>, String) {
    let bin_dir = workdir.join("bin");
    let log = workdir.join("sqlx-invocations.log");
    stub_sqlx(&bin_dir, &log);

    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let mut cmd = Command::new("sh");
    cmd.arg(manifest_path("docker/entrypoint.sh"))
        .current_dir(workdir)
        .env("PATH", path)
        .env_remove("DATABASE_URL")
        .args(args);
    for (key, value) in env {
        cmd.env(key, value);
    }

    let output = cmd.output().expect("spawn entrypoint");
    let recorded = fs::read_to_string(&log).unwrap_or_default();
    (output.status.code(), recorded)
}

fn write_database_config(dir: &Path) {
    write_database_config_with_password(dir, "s3cret");
}

fn write_database_config_with_password(dir: &Path, password: &str) {
    fs::write(
        dir.join("configuration.yaml"),
        format!(
            "app_host: 0.0.0.0\n\
             app_port: 8000\n\
             database:\n\
             \x20 host: db.internal\n\
             \x20 port: 5433\n\
             \x20 username: stacker\n\
             \x20 password: \"{password}\"\n\
             \x20 database_name: stacker\n\
             amqp:\n\
             \x20 host: mq.internal\n\
             \x20 port: 5672\n\
             \x20 username: guest\n\
             \x20 password: guest\n"
        ),
    )
    .expect("write configuration.yaml");
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("stacker-entrypoint-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn entrypoint_runs_sqlx_migrate_before_the_command() {
    let workdir = temp_dir("order");
    let (code, recorded) = run_entrypoint(
        &workdir,
        &[("DATABASE_URL", FIXTURE_DB_URL)],
        &["echo", "booted"],
    );

    assert_eq!(code, Some(0), "entrypoint should succeed");
    assert!(
        recorded.contains("ARGS:migrate run"),
        "expected `sqlx migrate run`, got: {recorded}"
    );
}

#[test]
fn entrypoint_forwards_migrations_directory() {
    let workdir = temp_dir("source");
    let (code, recorded) = run_entrypoint(
        &workdir,
        &[
            ("DATABASE_URL", FIXTURE_DB_URL),
            ("MIGRATIONS_DIR", "/app/migrations"),
        ],
        &["true"],
    );

    assert_eq!(code, Some(0), "entrypoint should succeed");
    assert!(
        recorded.contains("ARGS:migrate run --source /app/migrations"),
        "expected --source /app/migrations, got: {recorded}"
    );
}

#[test]
fn entrypoint_uses_database_url_from_the_environment() {
    let workdir = temp_dir("env-url");
    // Fixture value; nothing listens on it.
    let url = "postgres://user:pass@db:5432/envdb"; // pragma: allowlist secret
    let (code, recorded) = run_entrypoint(&workdir, &[("DATABASE_URL", url)], &["true"]);

    assert_eq!(code, Some(0), "entrypoint should succeed");
    assert!(
        recorded.contains(&format!("URL:{url}")),
        "expected the env DATABASE_URL to reach sqlx, got: {recorded}"
    );
}

#[test]
fn entrypoint_derives_database_url_from_configuration_yaml() {
    let workdir = temp_dir("config-url");
    write_database_config(&workdir);

    let (code, recorded) = run_entrypoint(
        &workdir,
        &[("CONFIGURATION_YAML", "configuration.yaml")],
        &["true"],
    );

    assert_eq!(
        code,
        Some(0),
        "entrypoint should derive DATABASE_URL from configuration.yaml, got: {recorded}"
    );
    // Fixture credentials from write_database_config(); the connection string
    // is asserted verbatim so a change to the URL shape fails loudly.
    let expected = "URL:postgres://stacker:s3cret@db.internal:5433/stacker"; // pragma: allowlist secret
    assert!(
        recorded.contains(expected),
        "expected the connection string built from configuration.yaml, got: {recorded}"
    );
}

#[test]
fn entrypoint_percent_encodes_credentials_in_the_derived_url() {
    let workdir = temp_dir("encoded-url");
    write_database_config_with_password(&workdir, "p#ss word@x/y");

    let (code, recorded) = run_entrypoint(
        &workdir,
        &[("CONFIGURATION_YAML", "configuration.yaml")],
        &["true"],
    );

    assert_eq!(
        code,
        Some(0),
        "entrypoint should derive DATABASE_URL from configuration.yaml, got: {recorded}"
    );
    // The server builds its own PgConnectOptions and tolerates raw credentials;
    // sqlx-cli only takes a URL, so the userinfo has to be percent-encoded or a
    // password with @ / : # silently points sqlx at the wrong host.
    let expected = "URL:postgres://stacker:p%23ss%20word%40x%2Fy@db.internal:5433/stacker"; // pragma: allowlist secret
    assert!(
        recorded.contains(expected),
        "expected percent-encoded credentials, got: {recorded}"
    );
}

#[test]
fn entrypoint_fails_closed_without_any_database_configuration() {
    let workdir = temp_dir("no-config");

    let (code, _) = run_entrypoint(&workdir, &[], &["true"]);

    assert_ne!(
        code,
        Some(0),
        "no DATABASE_URL and no configuration.yaml must stop the container, not skip migrations"
    );
}

#[test]
fn entrypoint_execs_the_requested_command_after_migrating() {
    let workdir = temp_dir("exec");
    let marker = workdir.join("booted.marker");

    let (code, _) = run_entrypoint(
        &workdir,
        &[("DATABASE_URL", FIXTURE_DB_URL)],
        &["sh", "-c", &format!("touch '{}'", marker.display())],
    );

    assert_eq!(code, Some(0), "entrypoint should succeed");
    assert!(
        marker.exists(),
        "the command given to the entrypoint must actually run"
    );
}

#[test]
fn production_image_ships_migrations_and_a_migrating_entrypoint() {
    let dockerfile = active_lines(&read("Dockerfile"));

    assert!(
        dockerfile.contains("COPY ./migrations ./migrations"),
        "production stage must copy ./migrations into the image"
    );
    assert!(
        dockerfile.contains("COPY ./docker/entrypoint.sh"),
        "production stage must copy the entrypoint script into the image"
    );
    assert!(
        dockerfile.contains(r#"ENTRYPOINT ["/app/entrypoint.sh"]"#),
        "production image must boot through the migrating entrypoint"
    );
    assert!(
        dockerfile.contains(r#"CMD ["/app/server"]"#),
        "production image must default to running the server through the entrypoint"
    );
}

#[test]
fn config_contract_migration_is_present() {
    let migrations = fs::read_to_string(
        manifest_path("migrations")
            .join("20260828121000_stack_template_version_config_contract.up.sql"),
    )
    .expect("the config_contract migration must exist in migrations/");

    assert!(
        migrations.contains("config_contract"),
        "migration 20260828121000 must define config_contract"
    );
}

#[test]
fn compose_files_do_not_boot_around_the_migration_entrypoint() {
    for compose in ["docker-compose.yml", "docker-compose.dev.yml"] {
        let contents = read(compose);
        for line in contents.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || !trimmed.starts_with("entrypoint:") {
                continue;
            }
            assert!(
                !trimmed.contains("/app/server"),
                "{compose} boots the server with {trimmed:?}, which skips migrations entirely"
            );
        }
    }
}
