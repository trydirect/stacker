//! Deploy helper endpoints for the 1-Click Deploy flow.
//!
//! Public endpoints under `/api/v1/deploy/*` used by the `user/` service to
//! prepare and start an immutable deploy from a badge deep link.
//!
//! - `POST /api/v1/deploy/validate` — parse + semantically validate an
//!   arbitrary `stacker.yml` (reuses the same `StackerConfig` logic the CLI
//!   uses). Never a 500 for a bad config: parse/semantic issues map to 422.
//! - `POST /api/v1/deploy/clone` — (protected) clone a baked snapshot on
//!   Hetzner with per-user env injected via cloud-init.

use actix_web::web::ServiceConfig;
use actix_web::{post, web, HttpResponse, Responder};
use serde::Serialize;
use std::collections::BTreeMap;

use crate::cli::config_parser::StackerConfig;
use crate::cli::error::Severity;

pub mod clone;

fn register(cfg: &mut ServiceConfig) {
    cfg.service(validate).service(clone::clone_server);
}

/// Mount `/api/v1/deploy/*` routes. Public `validate` (casbin
/// `group_anonymous`), protected `clone` (casbin `group_user`).
pub fn configure(cfg: &mut ServiceConfig) {
    cfg.service(web::scope("/api/v1/deploy").configure(register));
}

/// App-level composition preview returned for a valid stacker.yml.
#[derive(Debug, Serialize)]
struct Composition {
    app: Option<AppComposition>,
    services: Vec<ServiceComposition>,
}

#[derive(Debug, Serialize)]
struct AppComposition {
    #[serde(rename = "type")]
    app_type: String,
    image: Option<String>,
    ports: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ServiceComposition {
    name: String,
    image: String,
    ports: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ValidationIssueJson {
    severity: String,
    code: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    field: Option<String>,
}

impl ValidationIssueJson {
    fn from_issue(issue: &crate::cli::error::ValidationIssue) -> Self {
        Self {
            severity: match issue.severity {
                Severity::Error => "error".to_string(),
                Severity::Warning => "warning".to_string(),
                Severity::Info => "info".to_string(),
            },
            code: issue.code.clone(),
            message: issue.message.clone(),
            field: issue.field.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ValidateOk {
    valid: bool,
    name: String,
    version: Option<String>,
    composition: Composition,
}

#[derive(Debug, Serialize)]
struct ValidateFailed {
    valid: bool,
    errors: Vec<ValidationIssueJson>,
    warnings: Vec<ValidationIssueJson>,
}

fn compose_from(config: &StackerConfig) -> Composition {
    let app = if config.app_present {
        Some(AppComposition {
            app_type: format!("{:?}", config.app.app_type).to_lowercase(),
            image: config.app.image.clone(),
            ports: config.app.ports.clone(),
        })
    } else {
        None
    };

    let services = config
        .services
        .iter()
        .map(|service| ServiceComposition {
            name: service.name.clone(),
            image: service.image.clone(),
            ports: service.ports.clone(),
        })
        .collect();

    Composition { app, services }
}

fn to_issue_json(issues: &[&crate::cli::error::ValidationIssue]) -> Vec<ValidationIssueJson> {
    issues
        .iter()
        .map(|i| ValidationIssueJson::from_issue(i))
        .collect()
}

/// `POST /api/v1/deploy/validate`
///
/// Body: raw stacker.yml YAML (text/plain). Returns 200 with the parsed
/// composition when valid, 422 with structured issues when not.
#[post("/validate")]
async fn validate(body: String) -> impl Responder {
    // Parse. A malformed YAML is a config error, not a server error.
    let config = match StackerConfig::from_str(&body) {
        Ok(config) => config,
        Err(err) => {
            let failed = ValidateFailed {
                valid: false,
                errors: vec![ValidationIssueJson {
                    severity: "error".to_string(),
                    code: "PARSE".to_string(),
                    message: format!("invalid stacker.yml: {err}"),
                    field: None,
                }],
                warnings: Vec::new(),
            };
            return HttpResponse::UnprocessableEntity().json(failed);
        }
    };

    let issues = config.validate_semantics();
    let errors: Vec<_> = issues
        .iter()
        .filter(|issue| matches!(issue.severity, Severity::Error))
        .collect();
    let warnings: Vec<_> = issues
        .iter()
        .filter(|issue| matches!(issue.severity, Severity::Warning))
        .collect();

    if !errors.is_empty() {
        return HttpResponse::UnprocessableEntity().json(ValidateFailed {
            valid: false,
            errors: to_issue_json(&errors),
            warnings: to_issue_json(&warnings),
        });
    }

    HttpResponse::Ok().json(ValidateOk {
        valid: true,
        name: config.name.clone(),
        version: config.version.clone(),
        composition: compose_from(&config),
    })
}

/// Re-export `BTreeMap` for the clone handler's env payload typing.
pub(crate) type EnvMap = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test;

    fn valid_yaml() -> &'static str {
        r#"name: ai-workflows-v2
version: "1.0.0"
app:
  type: custom
  image: flowiseai/flowise:latest
  ports:
    - "3001:3000"
services:
  - name: ai-workflows-db
    image: postgres:16-alpine
  - name: ai-workflows-n8n
    image: n8nio/n8n:latest
    ports:
      - "5679:5678"
deploy:
  target: cloud
  cloud:
    provider: hetzner
    region: hel1
    size: cpx32
    public_ports: ["22", "80", "443"]
"#
    }

    fn invalid_yaml() -> &'static str {
        r#"name: broken
app:
  type: custom
deploy:
  target: cloud
"#
    }

    fn raw_yaml_malformed() -> &'static str {
        "name: [unclosed"
    }

    #[actix_web::test]
    async fn valid_config_returns_200_with_composition() {
        let app = actix_web::test::init_service(actix_web::App::new().service(validate)).await;
        let req = test::TestRequest::post()
            .uri("/validate")
            .set_payload(valid_yaml())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["valid"], true);
        assert_eq!(body["name"], "ai-workflows-v2");
        assert_eq!(body["version"], "1.0.0");
        assert_eq!(
            body["composition"]["app"]["image"],
            "flowiseai/flowise:latest"
        );
        let services = body["composition"]["services"].as_array().unwrap();
        assert_eq!(services.len(), 2);
        assert_eq!(services[0]["name"], "ai-workflows-db");
        assert_eq!(services[1]["name"], "ai-workflows-n8n");
    }

    #[actix_web::test]
    async fn invalid_config_returns_422_with_structured_errors() {
        let app = actix_web::test::init_service(actix_web::App::new().service(validate)).await;
        let req = test::TestRequest::post()
            .uri("/validate")
            .set_payload(invalid_yaml())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY
        );

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["valid"], false);
        assert!(body["errors"].as_array().unwrap().len() >= 1, "{body}");
        assert_eq!(body["errors"][0]["severity"], "error");
    }

    #[actix_web::test]
    async fn malformed_yaml_returns_422_not_500() {
        let app = actix_web::test::init_service(actix_web::App::new().service(validate)).await;
        let req = test::TestRequest::post()
            .uri("/validate")
            .set_payload(raw_yaml_malformed())
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::UNPROCESSABLE_ENTITY
        );

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["valid"], false);
        assert_eq!(body["errors"][0]["code"], "PARSE");
    }
}
