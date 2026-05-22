use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use stacker::cli::local_pipe_store::LocalPipeDocument;

fn contract_path(filename: &str) -> PathBuf {
    let shared = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../config/shared-fixtures/pipe-contract")
        .join(filename);
    if shared.exists() {
        return shared;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/contracts/pipe-contract")
        .join(filename)
}

fn load_contract(filename: &str) -> Value {
    let path = contract_path(filename);
    let content = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read contract {}: {}", path.display(), err));
    serde_json::from_str(&content)
        .unwrap_or_else(|err| panic!("failed to parse contract {}: {}", path.display(), err))
}

#[test]
fn local_pipe_fixture_round_trips_and_stays_adapter_only() {
    let fixture = load_contract("local_pipe.smtp_adapter.v1.json");
    let pipe: LocalPipeDocument =
        serde_json::from_value(fixture.clone()).expect("fixture should deserialize");

    pipe.validate().expect("fixture should validate");
    assert_eq!(pipe.target.selector, "smtp");
    assert!(pipe.instance.target_adapter.is_some());
    assert!(pipe.instance.target_container.is_none());
    assert!(pipe.instance.target_url.is_none());
    assert_eq!(
        pipe.instance
            .target_adapter
            .as_ref()
            .and_then(|adapter| adapter.config.as_ref())
            .and_then(|config| config.get("host"))
            .and_then(|value| value.as_str()),
        Some("smtp")
    );
}

#[test]
fn local_pipe_secret_ref_fixture_validates_without_plaintext_secrets() {
    let fixture = load_contract("local_pipe.smtp_adapter.secret_ref.v1.json");
    let pipe: LocalPipeDocument =
        serde_json::from_value(fixture).expect("secret-ref fixture should deserialize");

    pipe.validate()
        .expect("secret-ref fixture should validate without plaintext secrets");
    let password = pipe
        .instance
        .target_adapter
        .as_ref()
        .and_then(|adapter| adapter.config.as_ref())
        .and_then(|config| config.get("password"))
        .expect("password config should exist");
    assert_eq!(
        password["secret_ref"]["name"].as_str(),
        Some("SMTP_PASSWORD")
    );
}

#[test]
fn promotion_request_fixture_matches_local_pipe_projection() {
    let pipe_fixture = load_contract("local_pipe.smtp_adapter.v1.json");
    let expected = load_contract("remote_pipe.promote.smtp_adapter.request.json");
    let pipe: LocalPipeDocument =
        serde_json::from_value(pipe_fixture).expect("promotion source should deserialize");

    let actual = json!({
        "template_request": pipe.to_template_request(),
        "instance_request": pipe.to_instance_request(
            "dep-20260522".to_string(),
            "tpl-remote-smtp".to_string()
        )
    });

    assert_eq!(actual, expected);
    assert!(actual["instance_request"].get("target_container").is_none());
    assert!(actual["instance_request"].get("target_url").is_none());
}

#[test]
fn plaintext_secret_fixture_stays_rejected() {
    let fixture = load_contract("remote_pipe.secret_ref.rejected_plaintext.json");
    let expected = fixture["expected_error_contains"]
        .as_str()
        .expect("negative fixture must declare expected error");
    let pipe: LocalPipeDocument =
        serde_json::from_value(fixture["pipe"].clone()).expect("negative pipe should deserialize");

    let err = pipe
        .validate()
        .expect_err("plaintext secret fixture must fail");
    assert!(err.to_string().contains(expected));
}

#[test]
fn smtp_trigger_report_fixture_has_expected_delivery_shape() {
    let report = load_contract("remote_pipe.trigger.smtp_adapter.report.json");

    assert_eq!(report["type"].as_str(), Some("trigger_pipe"));
    assert_eq!(report["target_response"]["adapter"].as_str(), Some("smtp"));
    assert_eq!(report["target_response"]["delivered"].as_bool(), Some(true));
    assert_eq!(
        report["target_response"]["body"]["accepted_recipients"].as_i64(),
        Some(1)
    );
}
