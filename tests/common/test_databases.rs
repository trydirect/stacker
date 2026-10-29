//! Drops the databases a test process created, when the process exits.
//!
//! Every spawned app gets its own database (about 11 MB with the migrations).
//! Left behind, a full run fills the CI runner's disk until Postgres panics
//! with "No space left on device". Most apps live in a `static` shared by a
//! test file, so nothing is dropped in Rust; the drop happens at process exit
//! instead. Under nextest that is the end of each test, under `cargo test`
//! the end of each test binary. A killed process still leaks its databases.
//!
//! Shared by the integration harness (`tests/common/mod.rs`) and the BDD
//! harness (`tests/bdd.rs`) via `#[path]` include.

use sqlx::{Connection, Executor, PgConnection};
use std::sync::Mutex;

/// (connection string without the database, database name)
static CREATED: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

extern "C" {
    fn atexit(callback: extern "C" fn()) -> std::os::raw::c_int;
}

/// Drop `name` when this process exits. Call right after `CREATE DATABASE`.
pub fn drop_at_exit(server: &str, name: &str) {
    let mut created = CREATED.lock().unwrap_or_else(|e| e.into_inner());
    if created.is_empty() {
        // SAFETY: registers a plain `extern "C"` function, as atexit expects.
        unsafe {
            atexit(drop_created);
        }
    }
    created.push((server.to_string(), name.to_string()));
}

extern "C" fn drop_created() {
    let created = std::mem::take(&mut *CREATED.lock().unwrap_or_else(|e| e.into_inner()));
    // A fresh thread: at exit the main thread's thread-locals, which tokio
    // needs, are already destroyed.
    let _ = std::thread::spawn(move || drop_all(created)).join();
}

fn drop_all(created: Vec<(String, String)>) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return;
    };
    runtime.block_on(async {
        for (server, name) in created {
            // FORCE: the app's server and pools still hold connections.
            let sql = format!(r#"DROP DATABASE IF EXISTS "{name}" WITH (FORCE)"#);
            let result = match PgConnection::connect(&server).await {
                Ok(mut conn) => conn.execute(sql.as_str()).await.map(|_| ()),
                Err(e) => Err(e),
            };
            if let Err(e) = result {
                eprintln!("test database {name} not dropped: {e}");
            }
        }
    });
}
