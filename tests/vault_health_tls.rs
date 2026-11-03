//! The Vault health check must trust the same CA as the Vault client.
//!
//! Vault behind a private CA works for every real call (the client loads
//! VAULT_CACERT), yet the health check used a bare HTTP client and reported
//! "certificate verify failed" forever, with nothing misconfigured.
//!
//! The test makes a private CA and a server certificate with the `openssl`
//! binary and serves HTTPS with `openssl s_server`; it is skipped where
//! `openssl` is not installed.

use stacker::health::{ComponentStatus, HealthChecker};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

fn openssl(dir: &Path, args: &[&str]) -> bool {
    Command::new("openssl")
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// A private CA and a server certificate for localhost signed by it.
fn make_certs(dir: &Path) -> bool {
    std::fs::write(
        dir.join("san.ext"),
        "subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\nkeyUsage=digitalSignature,keyEncipherment\n",
    )
    .unwrap();
    openssl(
        dir,
        &[
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "2",
            "-subj",
            "/CN=Test Private CA",
        ],
    ) && openssl(
        dir,
        &[
            "req",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
            "-subj",
            "/CN=localhost",
        ],
    ) && openssl(
        dir,
        &[
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-out",
            "server.pem",
            "-days",
            "2",
            "-extfile",
            "san.ext",
        ],
    )
}

struct Server(Child);

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

fn serve_https(dir: &Path) -> Option<(Server, u16)> {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let child = Command::new("openssl")
        .args([
            "s_server",
            "-accept",
            &port.to_string(),
            "-cert",
            "server.pem",
            "-key",
            "server.key",
            "-www",
            "-quiet",
        ])
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let server = Server(child);
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Some((server, port));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

async fn vault_status(address: &str, ca_cert: Option<String>) -> (ComponentStatus, String) {
    let mut settings = stacker::configuration::get_configuration().expect("configuration");
    settings.vault.address = address.to_string();
    settings.vault.ca_cert = ca_cert;
    settings.vault.client_cert = None;
    settings.vault.client_key = None;
    let pool = sqlx::PgPool::connect_lazy("postgres://unused@127.0.0.1:1/unused").unwrap();
    let checker = HealthChecker::new(Arc::new(pool), Arc::new(settings));
    let response = checker.check_all().await;
    let vault = response
        .components
        .get("vault")
        .expect("vault component")
        .clone();
    (vault.status, vault.message.unwrap_or_default())
}

#[tokio::test]
async fn the_vault_health_check_trusts_the_configured_private_ca() {
    let dir = tempfile::tempdir().unwrap();
    if !make_certs(dir.path()) {
        eprintln!("Skipping: openssl is not available");
        return;
    }
    let Some((_server, port)) = serve_https(dir.path()) else {
        eprintln!("Skipping: could not start openssl s_server");
        return;
    };
    let address = format!("https://localhost:{port}");
    let ca = std::fs::read_to_string(dir.path().join("ca.pem")).unwrap();

    let (status, message) = vault_status(&address, Some(ca)).await;
    // TLS wording differs by platform (OpenSSL, macOS Security), so check
    // for the connection failure itself rather than its text.
    assert!(
        !message.contains("error sending request"),
        "with the CA configured the connection must succeed: {message}"
    );
    assert!(
        status == ComponentStatus::Healthy || message.contains("slowly"),
        "with the CA configured Vault must read healthy: {status:?} {message}"
    );

    // Without the CA it still reports the TLS failure, so a real
    // misconfiguration stays visible.
    let (_, message) = vault_status(&address, None).await;
    assert!(
        message.contains("error sending request"),
        "without the CA the TLS failure must be reported: {message}"
    );
}
