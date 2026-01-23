/// Unit tests for Server model
/// Run: cargo t model_server -- --nocapture --show-output

use stacker::models::Server;

#[test]
fn test_server_default_values() {
    let server = Server::default();

    // Check default connection mode
    assert_eq!(server.connection_mode, "ssh", "Default connection mode should be 'ssh'");

    // Check default key status
    assert_eq!(server.key_status, "none", "Default key status should be 'none'");

    // Check optional fields are None
    assert!(server.vault_key_path.is_none(), "vault_key_path should be None by default");
    assert!(server.name.is_none(), "name should be None by default");
}

#[test]
fn test_server_serialization() {
    let server = Server {
        id: 1,
        user_id: "user123".to_string(),
        project_id: 10,
        region: Some("us-east-1".to_string()),
        zone: Some("a".to_string()),
        server: Some("c5.large".to_string()),
        os: Some("ubuntu-22.04".to_string()),
        disk_type: Some("ssd".to_string()),
        srv_ip: Some("192.168.1.1".to_string()),
        ssh_port: Some(22),
        ssh_user: Some("root".to_string()),
        vault_key_path: Some("users/user123/servers/1/ssh".to_string()),
        connection_mode: "ssh".to_string(),
        key_status: "active".to_string(),
        name: Some("Production Server".to_string()),
        ..Default::default()
    };

    // Test serialization to JSON
    let json = serde_json::to_string(&server);
    assert!(json.is_ok(), "Server should serialize to JSON");

    let json_str = json.unwrap();
    assert!(json_str.contains("\"connection_mode\":\"ssh\""));
    assert!(json_str.contains("\"key_status\":\"active\""));
    assert!(json_str.contains("\"name\":\"Production Server\""));
}

#[test]
fn test_server_deserialization() {
    let json = r#"{
        "id": 1,
        "user_id": "user123",
        "project_id": 10,
        "region": "us-west-2",
        "zone": null,
        "server": "t3.medium",
        "os": "debian-11",
        "disk_type": "hdd",
        "created_at": "2026-01-23T10:00:00Z",
        "updated_at": "2026-01-23T10:00:00Z",
        "srv_ip": "10.0.0.1",
        "ssh_port": 2222,
        "ssh_user": "admin",
        "vault_key_path": "users/user123/servers/1/ssh",
        "connection_mode": "ssh",
        "key_status": "pending",
        "name": "Staging"
    }"#;

    let server: Result<Server, _> = serde_json::from_str(json);
    assert!(server.is_ok(), "Server should deserialize from JSON");

    let s = server.unwrap();
    assert_eq!(s.connection_mode, "ssh");
    assert_eq!(s.key_status, "pending");
    assert_eq!(s.name, Some("Staging".to_string()));
    assert_eq!(s.ssh_port, Some(2222));
}

#[test]
fn test_server_key_status_values() {
    // Valid key status values
    let valid_statuses = ["none", "pending", "active", "failed"];

    for status in valid_statuses.iter() {
        let server = Server {
            key_status: status.to_string(),
            ..Default::default()
        };
        assert_eq!(&server.key_status, *status);
    }
}

#[test]
fn test_server_connection_mode_values() {
    // Valid connection modes
    let valid_modes = ["ssh", "password"];

    for mode in valid_modes.iter() {
        let server = Server {
            connection_mode: mode.to_string(),
            ..Default::default()
        };
        assert_eq!(&server.connection_mode, *mode);
    }
}
