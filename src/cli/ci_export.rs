//! CI/CD pipeline template generation.
//!
//! Uses `tera` (already in deps) to render platform-specific pipeline YAML from
//! the current `StackerConfig`.  Templates are embedded as string constants so
//! the binary requires no external template files.

use tera::{Context, Tera};

use crate::cli::config_parser::StackerConfig;
use crate::cli::error::CliError;

// ── Embedded templates ──────────────────────────────────────────────────────

/// GitHub Actions workflow template.
///
/// GitHub's own `${{ }}` expressions are wrapped in `{% raw %}…{% endraw %}`
/// so Tera does not attempt to evaluate them.
const GITHUB_ACTIONS_TEMPLATE: &str = r#"name: Deploy {{ name }} with Stacker

on:
  push:
    branches: [main]
  workflow_dispatch:

jobs:
  deploy:
    name: Deploy {{ name }}
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Stacker CLI
        run: curl -fsSL https://get.try.direct/stacker | sh

      - name: Deploy stack
        env:
          STACKER_TOKEN: {% raw %}${{ secrets.STACKER_TOKEN }}{% endraw %}
        run: stacker deploy --target {{ deploy_target }}
"#;

/// GitLab CI/CD pipeline template.
///
/// GitLab uses `$VARIABLE` syntax which does not conflict with Tera's `{{ }}`.
const GITLAB_CI_TEMPLATE: &str = r#"stages:
  - deploy

deploy:
  stage: deploy
  image: docker:latest
  services:
    - docker:dind
  variables:
    STACKER_TOKEN: $STACKER_TOKEN
  before_script:
    - curl -fsSL https://get.try.direct/stacker | sh
  script:
    - stacker deploy --target {{ deploy_target }}
  only:
    - main
"#;

// ── CiExporter ──────────────────────────────────────────────────────────────

/// Renders CI/CD pipeline YAML for a given `StackerConfig`.
pub struct CiExporter {
    config: StackerConfig,
}

impl CiExporter {
    pub fn new(config: StackerConfig) -> Self {
        Self { config }
    }

    /// Render a GitHub Actions workflow YAML string.
    pub fn generate_github(&self) -> Result<String, CliError> {
        self.render(GITHUB_ACTIONS_TEMPLATE)
    }

    /// Render a GitLab CI/CD pipeline YAML string.
    pub fn generate_gitlab(&self) -> Result<String, CliError> {
        self.render(GITLAB_CI_TEMPLATE)
    }

    fn render(&self, template_src: &str) -> Result<String, CliError> {
        let mut tera = Tera::default();
        tera.add_raw_template("ci", template_src)
            .map_err(|e| CliError::ConfigValidation(format!("CI template parse error: {e}")))?;

        let mut ctx = Context::new();
        ctx.insert("name", &self.config.name);
        ctx.insert("app_type", &self.config.app.app_type.to_string());
        ctx.insert("deploy_target", &self.config.deploy.target.to_string());

        if let Some(cloud) = &self.config.deploy.cloud {
            ctx.insert("cloud_provider", &cloud.provider.to_string());
            if let Some(region) = &cloud.region {
                ctx.insert("cloud_region", region);
            }
        }

        tera.render("ci", &ctx)
            .map_err(|e| CliError::ConfigValidation(format!("CI template render error: {e}")))
    }
}
