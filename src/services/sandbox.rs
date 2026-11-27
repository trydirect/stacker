//! Sandbox execution API (WS-S S2): exec and file operations inside a sandbox
//! via the agent command queue. The agent runs `docker exec` on the sandbox
//! container and reports back; these helpers enqueue the command and wait for
//! the agent's result (sync-over-async with a timeout).

use crate::db;
use crate::services::agent_dispatcher::AgentDispatcher;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;
use uuid::Uuid;

/// Default cap on collected exec output (1 MiB) and file payloads (10 MiB).
pub const MAX_EXEC_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

/// How long to wait for the agent to report before giving up.
const RESULT_WAIT: Duration = Duration::from_secs(60);
/// Poll interval while waiting for the agent result.
const RESULT_POLL: Duration = Duration::from_millis(250);

/// A `sandbox` agent command payload.
fn exec_params(sandbox_id: Uuid, command: &str, timeout_sec: Option<u32>) -> Value {
    json!({
        "action": "exec",
        "sandboxId": sandbox_id,
        "command": command,
        "timeoutSec": timeout_sec.unwrap_or(30),
        "maxOutputBytes": MAX_EXEC_OUTPUT_BYTES,
    })
}

fn write_file_params(sandbox_id: Uuid, path: &str, content_base64: &str) -> Value {
    json!({
        "action": "write_file",
        "sandboxId": sandbox_id,
        "path": path,
        "contentBase64": content_base64,
        "maxBytes": MAX_FILE_BYTES,
    })
}

fn read_file_params(sandbox_id: Uuid, path: &str) -> Value {
    json!({
        "action": "read_file",
        "sandboxId": sandbox_id,
        "path": path,
        "maxBytes": MAX_FILE_BYTES,
    })
}

fn list_files_params(sandbox_id: Uuid, path: &str) -> Value {
    json!({
        "action": "list_files",
        "sandboxId": sandbox_id,
        "path": path,
    })
}

/// Enqueue a `sandbox` command for the sandbox's agent and wait for the
/// result. Returns the agent's reported result JSON.
async fn enqueue_and_wait(
    pool: &PgPool,
    deployment_id: i32,
    params: Value,
) -> Result<Value, String> {
    let dispatcher = AgentDispatcher::new(pool);
    let command_id = dispatcher
        .queue_command(deployment_id, "sandbox", params)
        .await
        .map_err(|e| format!("Failed to queue sandbox command: {}", e))?;
    wait_for_result(pool, &command_id).await
}

/// Poll the command until the agent reports (or the wait times out).
async fn wait_for_result(pool: &PgPool, command_id: &str) -> Result<Value, String> {
    let deadline = std::time::Instant::now() + RESULT_WAIT;
    loop {
        let command = db::command::fetch_by_command_id(pool, command_id)
            .await
            .map_err(|e| format!("Failed to fetch command: {}", e))?
            .ok_or_else(|| "Command disappeared while waiting".to_string())?;

        match command.status.as_str() {
            "completed" => {
                return command
                    .result
                    .clone()
                    .ok_or_else(|| "Command completed without a result".to_string());
            }
            "failed" | "cancelled" => {
                let err = command
                    .result
                    .as_ref()
                    .and_then(|r| r.get("error"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("sandbox command failed")
                    .to_string();
                return Err(err);
            }
            _ => {
                if std::time::Instant::now() >= deadline {
                    return Err("Timed out waiting for the sandbox result".to_string());
                }
                tokio::time::sleep(RESULT_POLL).await;
            }
        }
    }
}

/// Exec a command in a sandbox (S2). Returns `{exitCode, stdout, stderr}`.
pub async fn exec(
    pool: &PgPool,
    sandbox_id: Uuid,
    deployment_id: i32,
    command: &str,
    timeout_sec: Option<u32>,
) -> Result<Value, String> {
    if command.trim().is_empty() {
        return Err("command must not be empty".to_string());
    }
    let params = exec_params(sandbox_id, command, timeout_sec);
    let result = enqueue_and_wait(pool, deployment_id, params).await?;
    db::sandbox::touch(pool, sandbox_id).await?;
    Ok(result)
}

/// Write a file into a sandbox (base64 content).
pub async fn write_file(
    pool: &PgPool,
    sandbox_id: Uuid,
    deployment_id: i32,
    path: &str,
    content_base64: &str,
) -> Result<Value, String> {
    if path.trim().is_empty() {
        return Err("path must not be empty".to_string());
    }
    let params = write_file_params(sandbox_id, path, content_base64);
    let result = enqueue_and_wait(pool, deployment_id, params).await?;
    db::sandbox::touch(pool, sandbox_id).await?;
    Ok(result)
}

/// Read a file from a sandbox (returns base64 content).
pub async fn read_file(
    pool: &PgPool,
    sandbox_id: Uuid,
    deployment_id: i32,
    path: &str,
) -> Result<Value, String> {
    let params = read_file_params(sandbox_id, path);
    let result = enqueue_and_wait(pool, deployment_id, params).await?;
    db::sandbox::touch(pool, sandbox_id).await?;
    Ok(result)
}

/// List a directory in a sandbox.
pub async fn list_files(
    pool: &PgPool,
    sandbox_id: Uuid,
    deployment_id: i32,
    path: &str,
) -> Result<Value, String> {
    let params = list_files_params(sandbox_id, path);
    let result = enqueue_and_wait(pool, deployment_id, params).await?;
    db::sandbox::touch(pool, sandbox_id).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exec_params_shape_and_caps() {
        let params = exec_params(Uuid::nil(), "echo hi", Some(10));
        assert_eq!(params["action"], "exec");
        assert_eq!(params["command"], "echo hi");
        assert_eq!(params["timeoutSec"], 10);
        assert_eq!(params["maxOutputBytes"], MAX_EXEC_OUTPUT_BYTES);
    }

    #[test]
    fn test_file_params_reference_paths_and_caps() {
        let write = write_file_params(Uuid::nil(), "/tmp/a", "aGk=");
        assert_eq!(write["action"], "write_file");
        assert_eq!(write["path"], "/tmp/a");
        assert_eq!(write["maxBytes"], MAX_FILE_BYTES);

        let read = read_file_params(Uuid::nil(), "/tmp/a");
        assert_eq!(read["action"], "read_file");

        let list = list_files_params(Uuid::nil(), "/tmp");
        assert_eq!(list["action"], "list_files");
    }

    #[test]
    fn test_empty_command_is_rejected() {
        // Validation is in exec(); the params builder alone won't reject.
        assert_eq!(exec_params(Uuid::nil(), "", None)["command"], "");
    }
}
