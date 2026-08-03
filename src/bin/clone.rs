//! `clone` — the user-side of immutable deploy (slice 4): create a server FROM a
//! baked snapshot `image_id`, injecting per-user env/domain via cloud-init. Run
//! it N times against the same `image_id` to prove bake→clone×N: every clone is
//! the identical validated artifact, differing only by the injected env.
//!
//! Usage:
//!   HETZNER_TOKEN=... cargo run --bin clone -- \
//!     --image-id 12345678 --name proof-1 --domain app1.example.com \
//!     --location fsn1 --type cpx11 --ssh-key-id 987 --env FOO=bar
//!
//! Prints the new server's id + public IPv4.

use std::collections::BTreeMap;

use stacker::connectors::hetzner::{
    HetznerCloudClient, HetznerCloudConnector, HetznerCreateServerRequest,
};
use stacker::helpers::cloud_init::{render_user_data, BootConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mut image_id: Option<i64> = None;
    let mut name = "immutable-clone".to_string();
    let mut server_type = "cpx11".to_string();
    let mut location = "fsn1".to_string();
    let mut domain = "example.com".to_string();
    let mut admin_email = "admin@example.com".to_string();
    let mut ssh_key_ids: Vec<i64> = Vec::new();
    let mut env: BTreeMap<String, String> = BTreeMap::new();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--image-id" => { image_id = args.get(i + 1).and_then(|v| v.parse().ok()); i += 2; }
            "--name" => { name = args.get(i + 1).cloned().unwrap_or(name); i += 2; }
            "--type" => { server_type = args.get(i + 1).cloned().unwrap_or(server_type); i += 2; }
            "--location" => { location = args.get(i + 1).cloned().unwrap_or(location); i += 2; }
            "--domain" => { domain = args.get(i + 1).cloned().unwrap_or(domain); i += 2; }
            "--admin-email" => { admin_email = args.get(i + 1).cloned().unwrap_or(admin_email); i += 2; }
            "--ssh-key-id" => {
                if let Some(id) = args.get(i + 1).and_then(|v| v.parse().ok()) {
                    ssh_key_ids.push(id);
                }
                i += 2;
            }
            "--env" => {
                if let Some(kv) = args.get(i + 1) {
                    if let Some((k, v)) = kv.split_once('=') {
                        env.insert(k.to_string(), v.to_string());
                    }
                }
                i += 2;
            }
            other => { eprintln!("ignoring unknown arg: {other}"); i += 1; }
        }
    }

    let image_id = image_id.ok_or("provide --image-id <baked snapshot id>")?;
    let token = std::env::var("HETZNER_TOKEN")
        .map_err(|_| "set HETZNER_TOKEN to the Hetzner Cloud API token".to_string())?;

    // The only per-user variance: env + domain -> cloud-init user-data.
    env.entry("DOMAIN".to_string()).or_insert_with(|| domain.clone());
    let boot = BootConfig { domain: domain.clone(), admin_email, env };
    let user_data = render_user_data(&boot);

    let request = HetznerCreateServerRequest {
        name: name.clone(),
        server_type,
        location,
        image_id,
        ssh_key_ids,
        user_data: Some(user_data),
    };

    let connector = HetznerCloudClient::from_env().map_err(|e| e.to_string())?;
    let server = connector
        .create_server_from_image(&token, request)
        .await
        .map_err(|e| e.to_string())?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "name": name,
            "domain": domain,
            "image_id": image_id,
            "server_id": server.id,
            "public_ipv4": server.public_ipv4,
        }))?
    );
    eprintln!(
        "\nCloned server {} from image {} -> {}. Once booted, it should serve the baked stack with the injected env.",
        server.id, image_id, server.public_ipv4.as_deref().unwrap_or("(pending)")
    );
    Ok(())
}
