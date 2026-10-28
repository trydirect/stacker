//! Every spawned test app creates its own database. Left behind, they fill the
//! CI runner's disk (about 11 MB each, one or more per test process) until
//! Postgres panics with "No space left on device" and the rest of the run
//! fails to start. A test process must drop the databases it created.

mod common;

use sqlx::{Connection, PgConnection};

const PREFIX: &str = "TEST_DATABASE=";

/// Runs only as the child of `a_finished_test_process_leaves_no_database`.
#[tokio::test]
#[ignore = "child process of a_finished_test_process_leaves_no_database"]
async fn spawn_an_app_and_report_its_database() {
    let Some(app) = common::spawn_app().await else {
        return;
    };
    println!("{PREFIX}{}", app.connection_string);
}

#[test]
fn a_finished_test_process_leaves_no_database() {
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "spawn_an_app_and_report_its_database",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .expect("run the child test process");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "child failed: {stdout}");
    let Some(connection_string) = stdout
        .lines()
        .find_map(|line| line.strip_prefix(PREFIX))
        .map(str::to_string)
    else {
        println!("Skipping: the child could not reach postgres");
        return;
    };
    let (server, name) = connection_string.rsplit_once('/').unwrap();

    let left = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut conn = PgConnection::connect(&format!("{server}/postgres"))
            .await
            .expect("connect to postgres");
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM pg_database WHERE datname = $1")
            .bind(name)
            .fetch_one(&mut conn)
            .await
            .expect("query pg_database")
    });
    assert_eq!(left, 0, "database {name} outlived the test process");
}
