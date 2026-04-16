use crate::steps::StepWorld;
use cucumber::{then, when};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio_tungstenite::tungstenite;

/// Helper: build a WS URL from the base HTTP URL
fn ws_url(base: &str, path: &str) -> String {
    base.replace("http://", "ws://") + path
}

/// Helper: connect to the MCP WebSocket with auth token
async fn connect_mcp(
    base_url: &str,
    token: &str,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tungstenite::Error,
> {
    let url = ws_url(base_url, "/mcp");
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Authorization", format!("Bearer {}", token))
        .header("Sec-WebSocket-Protocol", "mcp")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .header("Host", url.replace("ws://", ""))
        .body(())
        .unwrap();

    let (ws, _resp) = tokio_tungstenite::connect_async(request).await?;
    Ok(ws)
}

/// Helper: send a JSON-RPC message over WS and read the response
async fn send_and_recv(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    msg: &serde_json::Value,
) -> Option<serde_json::Value> {
    let text = serde_json::to_string(msg).unwrap();
    ws.send(tungstenite::Message::Text(text)).await.ok()?;

    let timeout = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await;
    match timeout {
        Ok(Some(Ok(tungstenite::Message::Text(resp)))) => serde_json::from_str(&resp).ok(),
        _ => None,
    }
}

// ---------- Connection Steps ----------

#[when("I connect to the MCP WebSocket endpoint")]
async fn connect_ws(world: &mut StepWorld) {
    match connect_mcp(&world.base_url, &world.auth_token).await {
        Ok(ws) => {
            world.mcp_connected = true;
            world.mcp_ws.0 = Some(ws);
        }
        Err(e) => {
            world.mcp_connected = false;
            eprintln!("MCP WS connect failed: {}", e);
        }
    }
}

#[when("I connect to the MCP WebSocket endpoint without auth")]
async fn connect_ws_no_auth(world: &mut StepWorld) {
    match connect_mcp(&world.base_url, "").await {
        Ok(ws) => {
            world.mcp_connected = true;
            world.mcp_ws.0 = Some(ws);
        }
        Err(_) => {
            world.mcp_connected = false;
        }
    }
}

#[then("the MCP connection should be open")]
async fn assert_connected(world: &mut StepWorld) {
    assert!(
        world.mcp_connected,
        "Expected MCP WebSocket connection to be open"
    );
}

#[then("the MCP connection should be rejected")]
async fn assert_rejected(world: &mut StepWorld) {
    assert!(
        !world.mcp_connected,
        "Expected MCP WebSocket connection to be rejected"
    );
}

// ---------- JSON-RPC Steps ----------

#[when("I send an MCP initialize request")]
async fn send_initialize(world: &mut StepWorld) {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "bdd-test", "version": "1.0.0"}
        }
    });
    let ws = world.mcp_ws.0.as_mut().expect("MCP WS not connected");
    world.mcp_response = send_and_recv(ws, &msg).await;
}

#[when(regex = r#"^I send an MCP tools/list request$"#)]
async fn send_tools_list(world: &mut StepWorld) {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let ws = world.mcp_ws.0.as_mut().expect("MCP WS not connected");
    world.mcp_response = send_and_recv(ws, &msg).await;
}

#[when(regex = r#"^I send an MCP tools/call request for "([^"]+)" with arguments:$"#)]
async fn send_tools_call(world: &mut StepWorld, step: &cucumber::gherkin::Step, tool_name: String) {
    let doc = step.docstring.as_deref().unwrap_or("{}");
    let arguments: serde_json::Value = serde_json::from_str(doc.trim()).unwrap_or(json!({}));
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": arguments
        }
    });
    let ws = world.mcp_ws.0.as_mut().expect("MCP WS not connected");
    world.mcp_response = send_and_recv(ws, &msg).await;
}

#[when(regex = r#"^I send an MCP request with method "([^"]+)" and no params$"#)]
async fn send_no_params(world: &mut StepWorld, method: String) {
    let msg = json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": method
    });
    let ws = world.mcp_ws.0.as_mut().expect("MCP WS not connected");
    world.mcp_response = send_and_recv(ws, &msg).await;
}

#[when(regex = r#"^I send raw MCP text "([^"]+)"$"#)]
async fn send_raw_text(world: &mut StepWorld, text: String) {
    let ws = world.mcp_ws.0.as_mut().expect("MCP WS not connected");
    ws.send(tungstenite::Message::Text(text)).await.ok();
    let timeout = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await;
    world.mcp_response = match timeout {
        Ok(Some(Ok(tungstenite::Message::Text(resp)))) => serde_json::from_str(&resp).ok(),
        _ => None,
    };
}

#[when(regex = r#"^I send an MCP notification "([^"]+)"$"#)]
async fn send_notification(world: &mut StepWorld, method: String) {
    let msg = json!({
        "jsonrpc": "2.0",
        "method": method
    });
    let ws = world.mcp_ws.0.as_mut().expect("MCP WS not connected");
    let text = serde_json::to_string(&msg).unwrap();
    ws.send(tungstenite::Message::Text(text)).await.ok();
    // Short timeout — we expect no response for notifications
    let timeout = tokio::time::timeout(std::time::Duration::from_millis(500), ws.next()).await;
    world.mcp_response = match timeout {
        Ok(Some(Ok(tungstenite::Message::Text(resp)))) => serde_json::from_str(&resp).ok(),
        _ => None,
    };
}

// ---------- Assertion Steps ----------

#[then("the MCP response should have result")]
async fn assert_has_result(world: &mut StepWorld) {
    let resp = world
        .mcp_response
        .as_ref()
        .expect("No MCP response received");
    assert!(
        resp.get("result").is_some(),
        "Expected 'result' in response, got: {}",
        resp
    );
}

#[then("the MCP response should have error")]
async fn assert_has_error(world: &mut StepWorld) {
    let resp = world
        .mcp_response
        .as_ref()
        .expect("No MCP response received");
    assert!(
        resp.get("error").is_some(),
        "Expected 'error' in response, got: {}",
        resp
    );
}

#[then(regex = r#"^the MCP result field "([^"]+)" should be "([^"]+)"$"#)]
async fn assert_result_field(world: &mut StepWorld, field_path: String, expected: String) {
    let resp = world.mcp_response.as_ref().expect("No MCP response");
    let result = resp.get("result").expect("No result in MCP response");

    // Support dotted paths like "serverInfo.name"
    let val = field_path
        .split('.')
        .fold(Some(result), |cur, key| cur.and_then(|v| v.get(key)));

    let actual = val
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("Field '{}' not found in result: {}", field_path, result));
    assert_eq!(
        actual, expected,
        "MCP result field '{}' mismatch",
        field_path
    );
}

#[then(regex = r#"^the MCP error code should be (-?\d+)$"#)]
async fn assert_error_code(world: &mut StepWorld, code: i64) {
    let resp = world.mcp_response.as_ref().expect("No MCP response");
    let error = resp.get("error").expect("No error in MCP response");
    let actual = error
        .get("code")
        .and_then(|v| v.as_i64())
        .expect("No code in error");
    assert_eq!(actual, code, "MCP error code mismatch");
}

#[then("the MCP result should contain a non-empty tools array")]
async fn assert_tools_non_empty(world: &mut StepWorld) {
    let resp = world.mcp_response.as_ref().expect("No MCP response");
    let result = resp.get("result").expect("No result in MCP response");
    let tools = result
        .get("tools")
        .and_then(|v| v.as_array())
        .expect("No tools array in result");
    assert!(!tools.is_empty(), "Expected non-empty tools array");
}

#[then("the MCP tool response should not be an error")]
async fn assert_tool_not_error(world: &mut StepWorld) {
    let resp = world.mcp_response.as_ref().expect("No MCP response");
    let result = resp.get("result").expect("No result in MCP response");
    let is_error = result.get("isError").and_then(|v| v.as_bool());
    assert_ne!(
        is_error,
        Some(true),
        "Expected tool call to succeed, got error: {}",
        result
    );
}

#[then(regex = r#"^no MCP response should be received within (\d+)ms$"#)]
async fn assert_no_response(world: &mut StepWorld, _millis: u64) {
    assert!(
        world.mcp_response.is_none(),
        "Expected no MCP response for notification, got: {:?}",
        world.mcp_response
    );
}
