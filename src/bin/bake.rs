//! `bake` — turn a deployed build box into a published snapshot (immutable
//! deploy, slice 2). You deploy a stack onto a throwaway Hetzner box the normal
//! way (`stacker install lamp` onto a fresh server), then run this to
//! health-gate it, snapshot it, and record the BakeRecord in the snapshot
//! registry (so `POST /api/v1/deploy/clone` can resolve the image_id).
//!
//! Usage:
//!   HETZNER_TOKEN=... DATABASE_URL=... cargo run --bin bake -- \
//!     --ip <build-box-ip> --stack ai-workflows-v2 --version 1.0.0 \
//!     --health-url http://<ip>/health
//!
//! `DATABASE_URL` (the stacker Postgres) persists the BakeRecord; without it
//! the bake still snapshots and prints the record, but it is not registered.

use stacker::connectors::hetzner::{HetznerCloudClient, HetznerSnapshotTarget};
use stacker::helpers::bake::run_bake;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut ip: Option<String> = None;
    let mut server_id: Option<i64> = None;
    let mut stack = "lamp".to_string();
    let mut version = "v1".to_string();
    let mut health_url: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--ip" => {
                ip = args.get(i + 1).cloned();
                i += 2;
            }
            "--server-id" => {
                server_id = args.get(i + 1).and_then(|v| v.parse().ok());
                i += 2;
            }
            "--stack" => {
                stack = args.get(i + 1).cloned().unwrap_or(stack);
                i += 2;
            }
            "--version" => {
                version = args.get(i + 1).cloned().unwrap_or(version);
                i += 2;
            }
            "--health-url" => {
                health_url = args.get(i + 1).cloned();
                i += 2;
            }
            other => {
                eprintln!("ignoring unknown arg: {other}");
                i += 1;
            }
        }
    }

    if ip.is_none() && server_id.is_none() {
        return Err("provide --ip <build-box-ip> or --server-id <id>".into());
    }

    let token = std::env::var("HETZNER_TOKEN")
        .map_err(|_| "set HETZNER_TOKEN to the Hetzner Cloud API token".to_string())?;

    // Health gate.
    let (healthy, detail) = match &health_url {
        Some(url) => match reqwest::get(url).await {
            Ok(r) if r.status().is_success() => (true, String::new()),
            Ok(r) => (false, format!("{url} returned {}", r.status())),
            Err(e) => (false, format!("{url} unreachable: {e}")),
        },
        None => {
            eprintln!("WARNING: no --health-url given; skipping health gate (assuming healthy)");
            (true, String::new())
        }
    };

    let target = HetznerSnapshotTarget {
        provider_server_id: server_id,
        server_name: None,
        public_ip: ip,
    };

    let connector = HetznerCloudClient::from_env().map_err(|e| e.to_string())?;
    let record = run_bake(
        &connector, &token, target, &stack, &version, healthy, &detail,
    )
    .await
    .map_err(|e| e.to_string())?;

    println!("{}", serde_json::to_string_pretty(&record)?);
    eprintln!(
        "\nBaked {}:{} -> Hetzner image_id={}. Clone it with create_server_from_image(image={}).",
        record.stack, record.version, record.image_id, record.image_id
    );

    // Persist into the snapshot registry so /api/v1/deploy/clone can resolve it.
    if let Ok(db_url) = std::env::var("DATABASE_URL") {
        let pool = sqlx::PgPool::connect(&db_url).await?;
        let row = stacker::db::baked_snapshot::record(
            &pool,
            &record.stack,
            &record.version,
            &record.provider,
            record.image_id,
            record.healthy,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
        eprintln!(
            "Registered snapshot in registry: id={} stack={}:{} image_id={}",
            row.id, row.stack, row.version, row.image_id
        );
    } else {
        eprintln!("WARNING: DATABASE_URL not set — bake NOT registered in the snapshot registry.");
    }

    Ok(())
}
