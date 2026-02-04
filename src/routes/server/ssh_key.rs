use crate::db;
use crate::helpers::{JsonResponse, VaultClient};
use crate::models;
use actix_web::{delete, get, post, web, Responder, Result};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;

/// Request body for uploading an existing SSH key pair
#[derive(Debug, Deserialize)]
pub struct UploadKeyRequest {
    pub public_key: String,
    pub private_key: String,
}

/// Response containing the public key for copying
#[derive(Debug, Clone, Default, Serialize)]
pub struct PublicKeyResponse {
    pub public_key: String,
    pub fingerprint: Option<String>,
}

/// Response for SSH key generation
#[derive(Debug, Clone, Default, Serialize)]
pub struct GenerateKeyResponse {
    pub public_key: String,
    pub fingerprint: Option<String>,
    pub message: String,
}

/// Helper to verify server ownership
async fn verify_server_ownership(
    pg_pool: &PgPool,
    server_id: i32,
    user_id: &str,
) -> Result<models::Server, actix_web::Error> {
    db::server::fetch(pg_pool, server_id)
        .await
        .map_err(|_err| JsonResponse::<models::Server>::build().internal_server_error(""))
        .and_then(|server| match server {
            Some(s) if s.user_id != user_id => {
                Err(JsonResponse::<models::Server>::build().not_found("Server not found"))
            }
            Some(s) => Ok(s),
            None => Err(JsonResponse::<models::Server>::build().not_found("Server not found")),
        })
}

/// Generate a new SSH key pair for a server
/// POST /server/{id}/ssh-key/generate
#[tracing::instrument(name = "Generate SSH key for server.")]
#[post("/{id}/ssh-key/generate")]
pub async fn generate_key(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let server_id = path.0;
    let server = verify_server_ownership(pg_pool.get_ref(), server_id, &user.id).await?;

    // Check if server already has an active key
    if server.key_status == "active" {
        return Err(JsonResponse::<GenerateKeyResponse>::build().bad_request(
            "Server already has an active SSH key. Delete it first to generate a new one.",
        ));
    }

    // Update status to pending
    db::server::update_ssh_key_status(pg_pool.get_ref(), server_id, None, "pending")
        .await
        .map_err(|e| JsonResponse::<GenerateKeyResponse>::build().internal_server_error(&e))?;

    // Generate SSH key pair
    let (public_key, private_key) = VaultClient::generate_ssh_keypair().map_err(|e| {
        tracing::error!("Failed to generate SSH keypair: {}", e);
        // Reset status on failure
        let _ = futures::executor::block_on(db::server::update_ssh_key_status(
            pg_pool.get_ref(),
            server_id,
            None,
            "failed",
        ));
        JsonResponse::<GenerateKeyResponse>::build()
            .internal_server_error("Failed to generate SSH key")
    })?;

    // Store in Vault
    let vault_path = vault_client
        .get_ref()
        .store_ssh_key(&user.id, server_id, &public_key, &private_key)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store SSH key in Vault: {}", e);
            let _ = futures::executor::block_on(db::server::update_ssh_key_status(
                pg_pool.get_ref(),
                server_id,
                None,
                "failed",
            ));
            JsonResponse::<GenerateKeyResponse>::build()
                .internal_server_error("Failed to store SSH key")
        })?;

    // Update server with vault path and active status
    db::server::update_ssh_key_status(pg_pool.get_ref(), server_id, Some(vault_path), "active")
        .await
        .map_err(|e| JsonResponse::<GenerateKeyResponse>::build().internal_server_error(&e))?;

    let response = GenerateKeyResponse {
        public_key,
        fingerprint: None, // TODO: Calculate fingerprint
        message:
            "SSH key generated successfully. Copy the public key to your server's authorized_keys."
                .to_string(),
    };

    Ok(JsonResponse::build()
        .set_item(Some(response))
        .ok("SSH key generated"))
}

/// Upload an existing SSH key pair for a server
/// POST /server/{id}/ssh-key/upload
#[tracing::instrument(name = "Upload SSH key for server.", skip(form))]
#[post("/{id}/ssh-key/upload")]
pub async fn upload_key(
    path: web::Path<(i32,)>,
    form: web::Json<UploadKeyRequest>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let server_id = path.0;
    let server = verify_server_ownership(pg_pool.get_ref(), server_id, &user.id).await?;

    // Check if server already has an active key
    if server.key_status == "active" {
        return Err(JsonResponse::<models::Server>::build().bad_request(
            "Server already has an active SSH key. Delete it first to upload a new one.",
        ));
    }

    // Validate keys (basic check)
    if !form.public_key.starts_with("ssh-") && !form.public_key.starts_with("ecdsa-") {
        return Err(JsonResponse::<models::Server>::build()
            .bad_request("Invalid public key format. Expected OpenSSH format."));
    }

    if !form.private_key.contains("PRIVATE KEY") {
        return Err(JsonResponse::<models::Server>::build()
            .bad_request("Invalid private key format. Expected PEM format."));
    }

    // Update status to pending
    db::server::update_ssh_key_status(pg_pool.get_ref(), server_id, None, "pending")
        .await
        .map_err(|e| JsonResponse::<models::Server>::build().internal_server_error(&e))?;

    // Store in Vault
    let vault_path = vault_client
        .get_ref()
        .store_ssh_key(&user.id, server_id, &form.public_key, &form.private_key)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store SSH key in Vault: {}", e);
            let _ = futures::executor::block_on(db::server::update_ssh_key_status(
                pg_pool.get_ref(),
                server_id,
                None,
                "failed",
            ));
            JsonResponse::<models::Server>::build().internal_server_error("Failed to store SSH key")
        })?;

    // Update server with vault path and active status
    let updated_server =
        db::server::update_ssh_key_status(pg_pool.get_ref(), server_id, Some(vault_path), "active")
            .await
            .map_err(|e| JsonResponse::<models::Server>::build().internal_server_error(&e))?;

    Ok(JsonResponse::build()
        .set_item(Some(updated_server))
        .ok("SSH key uploaded successfully"))
}

/// Get the public key for a server (for copying to authorized_keys)
/// GET /server/{id}/ssh-key/public
#[tracing::instrument(name = "Get public SSH key for server.")]
#[get("/{id}/ssh-key/public")]
pub async fn get_public_key(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let server_id = path.0;
    let server = verify_server_ownership(pg_pool.get_ref(), server_id, &user.id).await?;

    if server.key_status != "active" {
        return Err(JsonResponse::<PublicKeyResponse>::build()
            .not_found("No active SSH key found for this server"));
    }

    let public_key = vault_client
        .get_ref()
        .fetch_ssh_public_key(&user.id, server_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch public key from Vault: {}", e);
            JsonResponse::<PublicKeyResponse>::build()
                .internal_server_error("Failed to retrieve public key")
        })?;

    let response = PublicKeyResponse {
        public_key,
        fingerprint: None, // TODO: Calculate fingerprint
    };

    Ok(JsonResponse::build().set_item(Some(response)).ok("OK"))
}

/// Delete SSH key for a server (disconnect)
/// DELETE /server/{id}/ssh-key
#[tracing::instrument(name = "Delete SSH key for server.")]
#[delete("/{id}/ssh-key")]
pub async fn delete_key(
    path: web::Path<(i32,)>,
    user: web::ReqData<Arc<models::User>>,
    pg_pool: web::Data<PgPool>,
    vault_client: web::Data<VaultClient>,
) -> Result<impl Responder> {
    let server_id = path.0;
    let server = verify_server_ownership(pg_pool.get_ref(), server_id, &user.id).await?;

    if server.key_status == "none" {
        return Err(JsonResponse::<models::Server>::build()
            .bad_request("No SSH key to delete for this server"));
    }

    // Delete from Vault
    if let Err(e) = vault_client
        .get_ref()
        .delete_ssh_key(&user.id, server_id)
        .await
    {
        tracing::warn!("Failed to delete SSH key from Vault (may not exist): {}", e);
        // Continue anyway - the key might not exist in Vault
    }

    // Update server status
    let updated_server =
        db::server::update_ssh_key_status(pg_pool.get_ref(), server_id, None, "none")
            .await
            .map_err(|e| JsonResponse::<models::Server>::build().internal_server_error(&e))?;

    Ok(JsonResponse::build()
        .set_item(Some(updated_server))
        .ok("SSH key deleted successfully"))
}
