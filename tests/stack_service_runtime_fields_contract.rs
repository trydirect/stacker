//! Runtime settings of a `services:` entry in stacker.yml reach the compose
//! file on every path that renders one.
//!
//! The contract is mirrored from
//! ../config/shared-fixtures/api-contracts/stack-service-runtime-fields.json
//! to tests/contracts/stack-service-runtime-fields.contract.json.
//!
//! cli_compose: the compose the CLI generates and deploys.
//! compose_sync: the service written into the author's own compose file
//! (`deploy.compose_file`).
//! project_app: the project app the CLI registers (POST /project), from which
//! the server renders the compose for deploys started outside the CLI.

use serde_json::{json, Value};
use stacker::cli::config_parser::StackerConfig;
use stacker::cli::generator::compose::ComposeDefinition;
use stacker::cli::stacker_client::build_project_body;
use stacker::forms::project::ProjectForm;

fn contract() -> Value {
    serde_json::from_str(include_str!(
        "contracts/stack-service-runtime-fields.contract.json"
    ))
    .expect("contract JSON")
}

fn config_with(service: &Value) -> StackerConfig {
    let doc = json!({
        "name": "runtime-fields",
        "app": { "type": "custom", "image": "nginx:1.27" },
        "services": [service],
    });
    StackerConfig::from_str(&serde_yaml::to_string(&doc).unwrap())
        .unwrap_or_else(|e| panic!("stacker.yml with {service} must parse: {e}"))
}

/// Every key of `expected` is present in `actual` with the same value;
/// `networks_aliases` means: every network the service joins carries them.
fn assert_contains(path: &str, case: &str, actual: &Value, expected: &Value) {
    for (key, want) in expected.as_object().unwrap() {
        if key == "networks_aliases" {
            let networks = actual["networks"].as_object().unwrap_or_else(|| {
                panic!(
                    "{path}/{case}: networks with aliases expected, got {}",
                    actual["networks"]
                )
            });
            assert!(!networks.is_empty(), "{path}/{case}: no network");
            for (name, net) in networks {
                assert_eq!(
                    &net["aliases"], want,
                    "{path}/{case}: aliases on network {name}"
                );
            }
            continue;
        }
        let got = &actual[key];
        if want.is_object() {
            assert_contains(path, case, got, want);
        } else {
            assert_eq!(got, want, "{path}/{case}: `{key}`\nservice: {actual}");
        }
    }
}

fn cases() -> Vec<(String, Value, Value)> {
    contract()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_string(),
                c["stacker_yml"].clone(),
                c["compose"].clone(),
            )
        })
        .collect()
}

#[test]
fn cli_compose_carries_every_declared_setting() {
    for (case, service, expected) in cases() {
        let config = config_with(&service);
        let compose = ComposeDefinition::try_from(&config)
            .expect("compose")
            .render();
        let doc: Value = serde_yaml::from_str(&compose).expect("rendered compose is YAML");
        let name = service["name"].as_str().unwrap();
        assert_contains("cli_compose", &case, &doc["services"][name], &expected);
    }
}

#[test]
fn compose_sync_carries_every_declared_setting() {
    for (case, service, expected) in cases() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("compose.yml"), "services: {}\n").unwrap();
        let mut config = config_with(&service);
        config.deploy.compose_file = Some("compose.yml".into());
        let name = service["name"].as_str().unwrap().to_string();
        stacker::cli::compose_service_sync::sync_configured_compose_services(
            dir.path(),
            &config,
            &[name.clone()],
        )
        .unwrap_or_else(|e| panic!("compose_sync/{case}: {e}"));
        let written = std::fs::read_to_string(dir.path().join("compose.yml")).unwrap();
        let doc: Value = serde_yaml::from_str(&written).unwrap();
        assert_contains("compose_sync", &case, &doc["services"][&name], &expected);
    }
}

#[test]
fn project_app_carries_every_declared_setting() {
    for (case, service, expected) in cases() {
        let body = build_project_body(&config_with(&service));
        let form: ProjectForm = serde_json::from_value(json!({ "custom": body["custom"] }))
            .unwrap_or_else(|e| {
                panic!("project_app/{case}: the server must accept the registered app: {e}")
            });
        let services = form.custom.services().expect("server renders services");
        let name = service["name"].as_str().unwrap();
        let rendered = services
            .get(name)
            .cloned()
            .flatten()
            .unwrap_or_else(|| panic!("project_app/{case}: service `{name}` missing"));
        let actual = serde_json::to_value(
            serde_yaml::from_str::<Value>(&serde_yaml::to_string(&rendered).unwrap()).unwrap(),
        )
        .unwrap();
        assert_contains("project_app", &case, &actual, &expected);
    }
}

#[test]
fn an_unset_restart_keeps_each_paths_default() {
    let service = json!({ "name": "plain", "image": "redis:7.4.11-alpine" });
    let config = config_with(&service);
    let compose: Value =
        serde_yaml::from_str(&ComposeDefinition::try_from(&config).unwrap().render()).unwrap();
    assert_eq!(compose["services"]["plain"]["restart"], "unless-stopped");

    let body = build_project_body(&config);
    let apps = body["custom"]["service"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let app = apps
        .iter()
        .chain(body["custom"]["web"].as_array().into_iter().flatten())
        .find(|a| a["code"] == "plain")
        .expect("registered app");
    assert_eq!(app["restart"], "always");
}

#[test]
fn an_unknown_restart_value_is_refused() {
    let doc = json!({
        "name": "runtime-fields",
        "app": { "type": "custom", "image": "nginx:1.27" },
        "services": [{ "name": "x", "image": "redis:7", "restart": "sometimes" }],
    });
    let err = StackerConfig::from_str(&serde_yaml::to_string(&doc).unwrap())
        .expect_err("restart: sometimes must be refused");
    assert!(
        format!("{err}").contains("unless-stopped"),
        "error names the allowed values: {err}"
    );
}
