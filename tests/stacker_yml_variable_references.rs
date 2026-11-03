//! A stacker.yml value can name a variable (`${VAR}`). Filled, the value
//! arrives as a string; read without filling variables (deploy validation),
//! the reference is still there. Neither may break a field that is not a
//! string.

use stacker::cli::config_parser::StackerConfig;
use stacker::cli::error::Severity;

fn errors(config: &StackerConfig) -> Vec<String> {
    config
        .validate_semantics()
        .iter()
        .filter(|i| matches!(i.severity, Severity::Error))
        .map(|i| i.message.clone())
        .collect()
}

/// Every boolean and number of stacker.yml written as `${VAR}`, with the
/// value the deploy fills in.
fn filled_config() -> String {
    r#"
name: refs
app:
  type: node
  privileged: ${STACKER_REF_FALSE}
  healthcheck:
    test: "CMD true"
    retries: ${STACKER_REF_FOUR}
services:
  - name: db
    image: postgres:16
    privileged: ${STACKER_REF_TRUE}
    healthcheck:
      test: "CMD pg_isready"
      retries: ${STACKER_REF_FOUR}
proxy:
  type: nginx
  auto_detect: ${STACKER_REF_FALSE}
deploy:
  target: server
  server:
    host: 203.0.113.10
    port: ${STACKER_REF_PORT}
ai:
  enabled: ${STACKER_REF_TRUE}
  timeout: ${STACKER_REF_FOUR}
monitoring:
  status_panel: ${STACKER_REF_TRUE}
  metrics:
    enabled: ${STACKER_REF_TRUE}
    telegraf: ${STACKER_REF_TRUE}
  alerts:
    interval: ${STACKER_REF_FOUR}
    on_recovery: ${STACKER_REF_FALSE}
    target: { pipe: oncall }
hooks:
  notify: ${STACKER_REF_FALSE}
pipes:
  - name: p
    source: app
    target: db
    source_endpoint: GET /a
    target_endpoint: POST /b
    poll_interval: ${STACKER_REF_FOUR}
    retry: ${STACKER_REF_FOUR}
    retry_backoff_ms: ${STACKER_REF_FOUR}
    retry_backoff_max_ms: ${STACKER_REF_FOUR}
"#
    .to_string()
}

#[test]
fn filled_booleans_and_numbers_are_read_as_their_type() {
    std::env::set_var("STACKER_REF_TRUE", "true");
    std::env::set_var("STACKER_REF_FALSE", "false");
    std::env::set_var("STACKER_REF_FOUR", "4");
    std::env::set_var("STACKER_REF_PORT", "2222");
    let config = StackerConfig::from_str(&filled_config()).unwrap_or_else(|e| panic!("{e}"));

    assert!(!config.app.privileged);
    assert_eq!(config.app.healthcheck.as_ref().unwrap().retries, 4);
    assert!(config.services[0].privileged);
    assert_eq!(config.services[0].healthcheck.as_ref().unwrap().retries, 4);
    assert!(!config.proxy.auto_detect);
    assert_eq!(config.deploy.server.as_ref().unwrap().port, 2222);
    assert!(config.ai.enabled);
    assert_eq!(config.ai.timeout, 4);
    assert!(config.monitoring.status_panel);
    let metrics = config.monitoring.metrics.as_ref().unwrap();
    assert!(metrics.enabled && metrics.telegraf);
    let alerts = config.monitoring.alerts.as_ref().unwrap();
    assert_eq!(alerts.interval, 4);
    assert!(!alerts.on_recovery);
    assert!(!config.hooks.notify);
    let pipe = &config.pipes[0];
    assert_eq!(pipe.poll_interval, Some(4));
    assert_eq!(pipe.retry, Some(4));
    assert_eq!(pipe.retry_backoff_ms, Some(4));
    assert_eq!(pipe.retry_backoff_max_ms, Some(4));
}

#[test]
fn a_boolean_or_number_that_is_not_one_is_still_refused() {
    for (field, bad) in [
        ("privileged", "\"yes please\""),
        ("privileged", "\"1\""),
        (
            "healthcheck:\n    test: \"CMD true\"\n    retries",
            "\"four\"",
        ),
        (
            "healthcheck:\n    test: \"CMD true\"\n    retries",
            "\"-1\"",
        ),
    ] {
        let yaml = format!("name: refs\napp:\n  type: node\n  {field}: {bad}\n");
        assert!(
            StackerConfig::from_str_raw(&yaml).is_err(),
            "{field}: {bad} must be refused"
        );
    }
}

/// Compose allows a variable for any part of a port mapping; validation
/// without filling variables checks the rest and lets the variable through.
#[test]
fn port_mappings_with_variables_pass_validation() {
    let yaml = r#"
name: refs
app:
  type: static
  ports: ["${APP_PORT:-8080}:80", "${APP_PORT}"]
services:
  - name: db
    image: postgres:16
    ports: ["127.0.0.1:${DB_PORT}:5432", "${BIND_IP}:5433:5432", "${LOW}-${HIGH}:9000-9010/udp"]
deploy:
  target: cloud
  cloud:
    provider: hetzner
    public_ports: ["${PUBLIC_PORT}", "${PUBLIC_PORT}/tcp", "443"]
"#;
    let config = StackerConfig::from_str_raw(yaml).expect("parses");
    assert_eq!(errors(&config), Vec::<String>::new());
}

#[test]
fn the_rest_of_a_port_mapping_with_a_variable_is_still_checked() {
    for bad in [
        "${APP_PORT}:133342",
        "${APP_PORT}:80/http",
        "abc:${APP_PORT}",
    ] {
        let yaml = format!("name: refs\napp:\n  type: static\n  ports: [\"{bad}\"]\n");
        let config = StackerConfig::from_str_raw(&yaml).expect("parses");
        assert!(!errors(&config).is_empty(), "{bad} must be refused");
    }
    let yaml = r#"
name: refs
app:
  type: static
deploy:
  target: cloud
  cloud:
    provider: hetzner
    public_ports: ["${PUBLIC_PORT}/http"]
"#;
    let config = StackerConfig::from_str_raw(yaml).expect("parses");
    assert!(
        !errors(&config).is_empty(),
        "public port with a bad protocol must be refused"
    );
}
