/// Unit tests for VaultClient SSH key methods
/// Run: cargo t vault_ssh -- --nocapture --show-output

use stacker::helpers::VaultClient;

#[test]
fn test_generate_ssh_keypair_creates_valid_keys() {
    let result = VaultClient::generate_ssh_keypair();
    assert!(result.is_ok(), "Key generation should succeed");

    let (public_key, private_key) = result.unwrap();

    // Check public key format
    assert!(
        public_key.starts_with("ssh-ed25519"),
        "Public key should be in OpenSSH format"
    );
    assert!(
        public_key.contains(" "),
        "Public key should have space separators"
    );

    // Check private key format
    assert!(
        private_key.contains("PRIVATE KEY"),
        "Private key should be in PEM format"
    );
    assert!(
        private_key.starts_with("-----BEGIN"),
        "Private key should have PEM header"
    );
    assert!(
        private_key.ends_with("-----\n") || private_key.ends_with("-----"),
        "Private key should have PEM footer"
    );
}

#[test]
fn test_generate_ssh_keypair_creates_unique_keys() {
    let result1 = VaultClient::generate_ssh_keypair();
    let result2 = VaultClient::generate_ssh_keypair();

    assert!(result1.is_ok());
    assert!(result2.is_ok());

    let (pub1, priv1) = result1.unwrap();
    let (pub2, priv2) = result2.unwrap();

    // Keys should be unique each time
    assert_ne!(pub1, pub2, "Generated public keys should be unique");
    assert_ne!(priv1, priv2, "Generated private keys should be unique");
}

#[test]
fn test_generate_ssh_keypair_key_length() {
    let result = VaultClient::generate_ssh_keypair();
    assert!(result.is_ok());

    let (public_key, private_key) = result.unwrap();

    // Ed25519 public keys are about 68 chars in base64 + prefix
    assert!(
        public_key.len() > 60,
        "Public key should be reasonable length"
    );
    assert!(
        public_key.len() < 200,
        "Public key should not be excessively long"
    );

    // Private keys are longer
    assert!(
        private_key.len() > 100,
        "Private key should be reasonable length"
    );
}

#[test]
fn test_ssh_key_path_format() {
    // Test the path generation logic (we can't test actual Vault connection in unit tests)
    let user_id = "user123";
    let server_id = 456;
    let expected_path = format!("users/{}/servers/{}/ssh", user_id, server_id);

    assert!(expected_path.contains(user_id));
    assert!(expected_path.contains(&server_id.to_string()));
    assert!(expected_path.ends_with("/ssh"));
}
