use std::path::{Path, PathBuf};
use std::io::{self, Write};

use crate::cli::ai_client::{AiProvider, create_provider};
use crate::cli::config_parser::{AiConfig, AiProviderType, StackerConfig};
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

fn parse_ai_provider(s: &str) -> Result<AiProviderType, CliError> {
    let json = format!("\"{}\"", s.trim().to_lowercase());
    serde_json::from_str::<AiProviderType>(&json).map_err(|_| {
        CliError::ConfigValidation(
            "Unknown AI provider. Use: openai, anthropic, ollama, custom".to_string(),
        )
    })
}

fn prompt_line(prompt: &str) -> Result<String, CliError> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn prompt_with_default(prompt: &str, default: &str) -> Result<String, CliError> {
    let line = prompt_line(&format!("{} [{}]: ", prompt, default))?;
    if line.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(line)
    }
}

fn configure_ai_interactive(config_path: &str) -> Result<AiConfig, CliError> {
    let path = Path::new(config_path);
    if !path.exists() {
        return Err(CliError::ConfigNotFound {
            path: PathBuf::from(config_path),
        });
    }

    let mut config = StackerConfig::from_file(path)?;
    let current = config.ai.clone();

    eprintln!("AI interactive setup for {}", config_path);

    let provider_default = current.provider.to_string();
    let provider_input = prompt_with_default(
        "AI provider (openai|anthropic|ollama|custom)",
        &provider_default,
    )?;
    let provider = parse_ai_provider(&provider_input)?;

    let model_default = current.model.as_deref().unwrap_or("");
    let model_input = prompt_with_default("Model (empty = provider default)", model_default)?;
    let model = if model_input.trim().is_empty() {
        None
    } else {
        Some(model_input)
    };

    let api_key_default = current.api_key.as_deref().unwrap_or("");
    let api_key_input = prompt_with_default("API key (empty = keep/none)", api_key_default)?;
    let api_key = if api_key_input.trim().is_empty() {
        current.api_key.clone()
    } else {
        Some(api_key_input)
    };

    let endpoint_default = current.endpoint.as_deref().unwrap_or("http://localhost:11434");
    let endpoint_input = prompt_with_default("Endpoint", endpoint_default)?;
    let endpoint = if endpoint_input.trim().is_empty() {
        None
    } else {
        Some(endpoint_input)
    };

    let timeout_default = current.timeout.to_string();
    let timeout_input = prompt_with_default("Timeout seconds", &timeout_default)?;
    let timeout = timeout_input.parse::<u64>().unwrap_or(current.timeout);

    let tasks = if current.tasks.is_empty() {
        vec!["dockerfile".to_string(), "compose".to_string()]
    } else {
        current.tasks.clone()
    };

    config.ai = AiConfig {
        enabled: true,
        provider,
        model,
        api_key,
        endpoint,
        timeout,
        tasks,
    };

    let backup_path = format!("{}.bak", config_path);
    std::fs::copy(config_path, &backup_path)?;
    let yaml = serde_yaml::to_string(&config)
        .map_err(|e| CliError::ConfigValidation(format!("Failed to serialize config: {}", e)))?;
    std::fs::write(config_path, yaml)?;

    eprintln!("✓ AI configuration saved to {}", config_path);
    eprintln!("  Backup written to {}", backup_path);
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
    pub configure: bool,
}

impl AiAskCommand {
    pub fn new(question: String, context: Option<String>) -> Self {
        Self {
            question,
            context,
            configure: false,
        }
    }

    pub fn with_configure(mut self, configure: bool) -> Self {
        self.configure = configure;
        self
    }
}

impl CallableTrait for AiAskCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        let ai_config = if self.configure {
            configure_ai_interactive(DEFAULT_CONFIG_FILE)?
        } else {
            load_ai_config(DEFAULT_CONFIG_FILE)?
        };
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
