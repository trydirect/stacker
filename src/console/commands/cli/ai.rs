use std::path::{Path, PathBuf};

use crate::cli::ai_client::{AiProvider, create_provider};
use crate::cli::config_parser::{AiConfig, StackerConfig};
use crate::cli::error::CliError;
use crate::console::commands::CallableTrait;

const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

/// Load AI config from stacker.yml.
fn load_ai_config(config_path: &str) -> Result<AiConfig, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }
    let config = StackerConfig::from_file(path)?;
    if !config.ai.enabled {
        return Err(CliError::AiNotConfigured);
    }
    Ok(config.ai)
}

/// Build a prompt from the question and optional context file content.
pub fn build_ai_prompt(question: &str, context_content: Option<&str>) -> String {
    match context_content {
        Some(ctx) => format!(
            "Given the following context:\n\n```\n{}\n```\n\nQuestion: {}",
            ctx, question
        ),
        None => question.to_string(),
    }
}

/// Core AI ask logic, extracted for testability.
pub fn run_ai_ask(
    question: &str,
    context: Option<&str>,
    provider: &dyn AiProvider,
) -> Result<String, CliError> {
    let context_content = match context {
        Some(path) => {
            let p = Path::new(path);
            if !p.exists() {
                return Err(CliError::ConfigNotFound {
                    path: PathBuf::from(path),
                });
            }
            Some(std::fs::read_to_string(p)?)
        }
        None => None,
    };

    let prompt = build_ai_prompt(question, context_content.as_deref());
    provider.complete(&prompt, "")
}

/// `stacker ai ask "<question>" [--context <file>]`
///
/// Sends a question to the configured AI provider for assistance
/// with Dockerfile, docker-compose, or deployment troubleshooting.
pub struct AiAskCommand {
    pub question: String,
    pub context: Option<String>,
}

impl AiAskCommand {
    pub fn new(question: String, context: Option<String>) -> Self {
        Self { question, context }
    }
}

impl CallableTrait for AiAskCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ai_config = load_ai_config(DEFAULT_CONFIG_FILE)?;
        let provider = create_provider(&ai_config)?;
        let response = run_ai_ask(
            &self.question,
            self.context.as_deref(),
            provider.as_ref(),
        )?;
        println!("{}", response);
        Ok(())
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    struct MockProvider {
        response: String,
    }

    impl MockProvider {
        fn new(response: &str) -> Self {
            Self { response: response.to_string() }
        }
    }

    impl AiProvider for MockProvider {
        fn name(&self) -> &str { "mock" }
        fn complete(&self, _prompt: &str, _context: &str) -> Result<String, CliError> {
            Ok(self.response.clone())
        }
    }

    #[test]
    fn test_build_prompt_without_context() {
        let prompt = build_ai_prompt("How do I optimize my Dockerfile?", None);
        assert_eq!(prompt, "How do I optimize my Dockerfile?");
    }

    #[test]
    fn test_build_prompt_with_context() {
        let prompt = build_ai_prompt("Explain this", Some("FROM node:18\nRUN npm install"));
        assert!(prompt.contains("context"));
        assert!(prompt.contains("FROM node:18"));
        assert!(prompt.contains("Explain this"));
    }

    #[test]
    fn test_run_ai_ask_returns_response() {
        let provider = MockProvider::new("Use multi-stage builds for smaller images.");
        let result = run_ai_ask("How to optimize?", None, &provider).unwrap();
        assert_eq!(result, "Use multi-stage builds for smaller images.");
    }

    #[test]
    fn test_run_ai_ask_with_context_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let ctx_path = dir.path().join("Dockerfile");
        std::fs::write(&ctx_path, "FROM rust:1.75\nCOPY . .").unwrap();

        let provider = MockProvider::new("Looks good!");
        let result = run_ai_ask(
            "Review this",
            Some(ctx_path.to_str().unwrap()),
            &provider,
        ).unwrap();
        assert_eq!(result, "Looks good!");
    }

    #[test]
    fn test_run_ai_ask_missing_context_file_errors() {
        let provider = MockProvider::new("unreachable");
        let result = run_ai_ask("question", Some("/does/not/exist.txt"), &provider);
        assert!(result.is_err());
    }
}
