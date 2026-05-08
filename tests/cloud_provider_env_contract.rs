use serde_json::Value;
use stacker::cli::cloud_env;

/// Load the shared cloud env contract.
///
/// Note: This contract is mirrored from
/// ../config/shared-fixtures/cloud-provider-env-vars.json
/// to stacker/tests/contracts/cloud-provider-env-vars.contract.json
/// for reliable CI access.
fn load_contract() -> Value {
    let contract_json = include_str!("contracts/cloud-provider-env-vars.contract.json");
    serde_json::from_str(contract_json).expect("contract JSON should be valid")
}

fn as_str_vec(values: &Value) -> Vec<&str> {
    values
        .as_array()
        .expect("value should be an array")
        .iter()
        .map(|value| value.as_str().expect("array entry should be a string"))
        .collect()
}

fn extract_env_tokens(text: &str) -> Vec<&str> {
    text.split(|ch: char| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_'))
        .filter(|token| !token.is_empty() && token.contains('_'))
        .collect()
}

#[test]
fn cloud_env_contract_has_expected_metadata() {
    let contract = load_contract();

    assert_eq!(contract["title"].as_str(), Some("cloud-provider-env-vars"));
    assert_eq!(contract["_owner"].as_str(), Some("trydirect/config"));
}

#[test]
fn token_provider_env_lists_match_contract() {
    let contract = load_contract();

    assert_eq!(
        as_str_vec(&contract["providers"]["hetzner"]["acceptedTokenEnvOrder"]),
        cloud_env::HETZNER_TOKEN_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["digitalocean"]["acceptedTokenEnvOrder"]),
        cloud_env::DIGITALOCEAN_TOKEN_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["linode"]["acceptedTokenEnvOrder"]),
        cloud_env::LINODE_TOKEN_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["vultr"]["acceptedTokenEnvOrder"]),
        cloud_env::VULTR_TOKEN_ENV_VARS
    );
}

#[test]
fn aws_and_contabo_env_lists_match_contract() {
    let contract = load_contract();

    assert_eq!(
        as_str_vec(&contract["providers"]["aws"]["acceptedKeyEnvOrder"]),
        cloud_env::AWS_KEY_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["aws"]["acceptedSecretEnvOrder"]),
        cloud_env::AWS_SECRET_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["contabo"]["acceptedFieldEnvOrder"]["cloud_key"]),
        cloud_env::CONTABO_CLIENT_ID_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["contabo"]["acceptedFieldEnvOrder"]["cloud_token"]),
        cloud_env::CONTABO_CLIENT_SECRET_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["contabo"]["acceptedFieldEnvOrder"]["cloud_user"]),
        cloud_env::CONTABO_API_USER_ENV_VARS
    );
    assert_eq!(
        as_str_vec(&contract["providers"]["contabo"]["acceptedFieldEnvOrder"]["cloud_password"]),
        cloud_env::CONTABO_API_PASSWORD_ENV_VARS
    );
}

#[test]
fn install_service_field_mapping_matches_contract() {
    let contract = load_contract();

    for provider in ["hetzner", "digitalocean", "linode", "vultr"] {
        assert_eq!(
            contract["providers"][provider]["installField"].as_str(),
            Some("cloud_token")
        );
    }

    assert_eq!(
        contract["providers"]["aws"]["installKeyField"].as_str(),
        Some("cloud_key")
    );
    assert_eq!(
        contract["providers"]["aws"]["installSecretField"].as_str(),
        Some("cloud_secret")
    );
}

#[test]
fn cli_examples_match_contract() {
    let contract = load_contract();

    assert_eq!(
        contract["providers"]["hetzner"]["cliExample"].as_str(),
        Some(cloud_env::provider_cli_example("htz"))
    );
    assert_eq!(
        contract["providers"]["digitalocean"]["cliExample"].as_str(),
        Some(cloud_env::provider_cli_example("do"))
    );
    assert_eq!(
        contract["providers"]["linode"]["cliExample"].as_str(),
        Some(cloud_env::provider_cli_example("lo"))
    );
    assert_eq!(
        contract["providers"]["vultr"]["cliExample"].as_str(),
        Some(cloud_env::provider_cli_example("vu"))
    );
    assert_eq!(
        contract["providers"]["aws"]["cliExample"].as_str(),
        Some(cloud_env::provider_cli_example("aws"))
    );
}

#[test]
fn deprecated_env_names_do_not_reappear_in_examples_or_hints() {
    let contract = load_contract();

    for provider in [
        ("hetzner", "htz"),
        ("digitalocean", "do"),
        ("linode", "lo"),
        ("vultr", "vu"),
        ("aws", "aws"),
        ("contabo", "cnt"),
    ] {
        let deprecated = contract["providers"][provider.0]["deprecatedEnv"]
            .as_array()
            .expect("deprecatedEnv should be an array");
        let example = cloud_env::provider_cli_example(provider.1);
        let hint = cloud_env::provider_missing_credentials_hint(provider.1);
        let example_tokens = extract_env_tokens(example);
        let hint_tokens = extract_env_tokens(hint);

        for env_name in deprecated {
            let env_name = env_name
                .as_str()
                .expect("deprecatedEnv entry should be a string");
            assert!(
                !example_tokens.contains(&env_name),
                "example for {} must not contain deprecated env {}",
                provider.0,
                env_name
            );
            assert!(
                !hint_tokens.contains(&env_name),
                "hint for {} must not contain deprecated env {}",
                provider.0,
                env_name
            );
        }
    }
}
