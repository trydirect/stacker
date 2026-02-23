use crate::cli::config_parser::{AiConfig, AiProviderType, AppType};
use crate::cli::error::CliError;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Constants
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Default OpenAI-compatible endpoint.
pub const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

/// Default Anthropic endpoint.
pub const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// Default Ollama endpoint.
pub const OLLAMA_API_URL: &str = "http://localhost:11434/api/chat";

/// Ollama tags endpoint (for listing available models).
pub const OLLAMA_TAGS_URL: &str = "http://localhost:11434/api/tags";

/// Default model per provider when none is specified in config.
pub fn default_model(provider: AiProviderType) -> &'static str {
    match provider {
        AiProviderType::Openai => "gpt-4o",
        AiProviderType::Anthropic => "claude-sonnet-4-20250514",
        AiProviderType::Ollama => "llama3",
        AiProviderType::Custom => "default",
    }
}

/// Preferred Ollama models for stacker.yml generation, in priority order.
/// The first available model from this list will be used.
const OLLAMA_PREFERRED_MODELS: &[&str] = &[
    "llama3",
    "llama3.1",
    "llama3.2",
    "llama3:latest",
    "codellama",
    "mistral",
    "mixtral",
    "deepseek-r1",
    "deepseek-coder",
    "qwen2.5-coder",
    "qwen2.5",
    "phi3",
    "gemma2",
    "gpt-oss",
];

/// Default request timeout in seconds.
const DEFAULT_AI_TIMEOUT_SECS: u64 = 300;

/// Resolve the AI request timeout in seconds.
///
/// Priority: `STACKER_AI_TIMEOUT` env var > `AiConfig.timeout` value > 300s default.
/// A value of 0 means no timeout (unlimited).
pub fn resolve_timeout(config_timeout: u64) -> u64 {
    if let Ok(val) = std::env::var("STACKER_AI_TIMEOUT") {
        if let Ok(secs) = val.parse::<u64>() {
            return secs;
        }
    }
    if config_timeout > 0 {
        config_timeout
    } else {
        DEFAULT_AI_TIMEOUT_SECS
    }
}

/// Query the local Ollama instance for available models.
/// Returns a list of model names, or an empty vec if Ollama is unreachable.
pub fn list_ollama_models(base_url: Option<&str>) -> Vec<String> {
    let tags_url = base_url
        .map(|u| {
            // Convert chat endpoint to tags endpoint
            u.replace("/api/chat", "/api/tags")
                .replace("/api/generate", "/api/tags")
        })
        .unwrap_or_else(|| OLLAMA_TAGS_URL.to_string());

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let response = match client.get(&tags_url).send() {
        Ok(r) if r.status().is_success() => r,
        _ => return Vec::new(),
    };

    let json: serde_json::Value = match response.json() {
        Ok(j) => j,
        Err(_) => return Vec::new(),
    };

    json["models"]
        .as_array()
        .map(|models| {
            models
                .iter()
                .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Pick the best available Ollama model for config generation.
/// Checks the preferred list first, then falls back to the first available model.
/// Returns None if no models are available.
pub fn pick_ollama_model(base_url: Option<&str>) -> Option<String> {
    let available = list_ollama_models(base_url);
    if available.is_empty() {
        return None;
    }

    // Check preferred models in priority order
    for preferred in OLLAMA_PREFERRED_MODELS {
        for avail in &available {
            // Match base name (e.g. "deepseek-r1" matches "deepseek-r1:latest")
            let avail_base = avail.split(':').next().unwrap_or(avail);
            if avail_base == *preferred || avail == preferred {
                return Some(avail.clone());
            }
        }
    }

    // No preferred model found — use the first non-embedding model
    available
        .into_iter()
        .find(|m| !m.contains("embed"))
        .or_else(|| Some("llama3".to_string()))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AiProvider trait — abstraction over LLM backends (DIP)
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Abstraction for LLM completion providers.
///
/// Production: `OpenAiProvider`, `AnthropicProvider`, `OllamaProvider`.
/// Tests: `MockAiProvider` returns canned responses.
pub trait AiProvider: Send + Sync {
    /// Provider name for error reporting.
    fn name(&self) -> &str;

    /// Send a completion request and return the response text.
    fn complete(&self, prompt: &str, context: &str) -> Result<String, CliError>;
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OpenAiProvider — OpenAI / OpenAI-compatible APIs
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Calls the OpenAI Chat Completions API (or any compatible endpoint).
/// Also works with Azure OpenAI, Together AI, Groq, etc.
pub struct OpenAiProvider {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl OpenAiProvider {
    pub fn from_config(config: &AiConfig) -> Result<Self, CliError> {
        let api_key = config.api_key.clone().ok_or(CliError::AiProviderError {
            provider: "openai".to_string(),
            message: "api_key is required for OpenAI provider".to_string(),
        })?;

        Ok(Self {
            endpoint: config
                .endpoint
                .clone()
                .unwrap_or_else(|| OPENAI_API_URL.to_string()),
            api_key,
            model: config
                .model
                .clone()
                .unwrap_or_else(|| default_model(AiProviderType::Openai).to_string()),
            timeout_secs: resolve_timeout(config.timeout),
        })
    }
}

impl AiProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    fn complete(&self, prompt: &str, context: &str) -> Result<String, CliError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": context },
                { "role": "user", "content": prompt }
            ],
            "temperature": 0.3
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()
            .map_err(|e| CliError::AiProviderError {
                provider: "openai".to_string(),
                message: format!("Failed to build HTTP client: {}", e),
            })?;

        let response = client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| CliError::AiProviderError {
                provider: "openai".to_string(),
                message: format!("Request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(CliError::AiProviderError {
                provider: "openai".to_string(),
                message: format!("HTTP {} — {}", status, text),
            });
        }

        let json: serde_json::Value = response.json().map_err(|e| CliError::AiProviderError {
            provider: "openai".to_string(),
            message: format!("Failed to parse response: {}", e),
        })?;

        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CliError::AiProviderError {
                provider: "openai".to_string(),
                message: "No content in response".to_string(),
            })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// AnthropicProvider — Claude API
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Calls the Anthropic Messages API.
pub struct AnthropicProvider {
    pub endpoint: String,
    pub api_key: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl AnthropicProvider {
    pub fn from_config(config: &AiConfig) -> Result<Self, CliError> {
        let api_key = config.api_key.clone().ok_or(CliError::AiProviderError {
            provider: "anthropic".to_string(),
            message: "api_key is required for Anthropic provider".to_string(),
        })?;

        Ok(Self {
            endpoint: config
                .endpoint
                .clone()
                .unwrap_or_else(|| ANTHROPIC_API_URL.to_string()),
            api_key,
            model: config
                .model
                .clone()
                .unwrap_or_else(|| default_model(AiProviderType::Anthropic).to_string()),
            timeout_secs: resolve_timeout(config.timeout),
        })
    }
}

impl AiProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn complete(&self, prompt: &str, context: &str) -> Result<String, CliError> {
        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 4096,
            "system": context,
            "messages": [
                { "role": "user", "content": prompt }
            ]
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()
            .map_err(|e| CliError::AiProviderError {
                provider: "anthropic".to_string(),
                message: format!("Failed to build HTTP client: {}", e),
            })?;

        let response = client
            .post(&self.endpoint)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| CliError::AiProviderError {
                provider: "anthropic".to_string(),
                message: format!("Request failed: {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(CliError::AiProviderError {
                provider: "anthropic".to_string(),
                message: format!("HTTP {} — {}", status, text),
            });
        }

        let json: serde_json::Value = response.json().map_err(|e| CliError::AiProviderError {
            provider: "anthropic".to_string(),
            message: format!("Failed to parse response: {}", e),
        })?;

        json["content"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CliError::AiProviderError {
                provider: "anthropic".to_string(),
                message: "No content in response".to_string(),
            })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// OllamaProvider — local Ollama instance
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Calls a local Ollama chat API. No API key required.
pub struct OllamaProvider {
    pub endpoint: String,
    pub model: String,
    pub timeout_secs: u64,
}

impl OllamaProvider {
    pub fn from_config(config: &AiConfig) -> Self {
        let endpoint = config
            .endpoint
            .clone()
            .unwrap_or_else(|| OLLAMA_API_URL.to_string());

        let model = config.model.clone().unwrap_or_else(|| {
            // Try to auto-detect the best available model
            match pick_ollama_model(Some(&endpoint)) {
                Some(m) => {
                    eprintln!("  Using Ollama model: {}", m);
                    m
                }
                None => {
                    let default = default_model(AiProviderType::Ollama).to_string();
                    eprintln!("  No models detected, trying default: {}", default);
                    default
                }
            }
        });

        let timeout_secs = resolve_timeout(config.timeout);

        Self { endpoint, model, timeout_secs }
    }
}

impl AiProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn complete(&self, prompt: &str, context: &str) -> Result<String, CliError> {
        let body = serde_json::json!({
            "model": self.model,
            "stream": false,
            "messages": [
                { "role": "system", "content": context },
                { "role": "user", "content": prompt }
            ]
        });

        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(self.timeout_secs))
            .build()
            .map_err(|e| CliError::AiProviderError {
                provider: "ollama".to_string(),
                message: format!("Failed to build HTTP client: {}", e),
            })?;

        let response = client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| CliError::AiProviderError {
                provider: "ollama".to_string(),
                message: format!("Request failed (is Ollama running?): {}", e),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().unwrap_or_default();
            return Err(CliError::AiProviderError {
                provider: "ollama".to_string(),
                message: format!("HTTP {} — {}", status, text),
            });
        }

        let json: serde_json::Value = response.json().map_err(|e| CliError::AiProviderError {
            provider: "ollama".to_string(),
            message: format!("Failed to parse response: {}", e),
        })?;

        json["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CliError::AiProviderError {
                provider: "ollama".to_string(),
                message: "No content in response".to_string(),
            })
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Provider factory
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Create the appropriate provider from an `AiConfig`.
/// Returns `AiNotConfigured` if AI is disabled.
pub fn create_provider(config: &AiConfig) -> Result<Box<dyn AiProvider>, CliError> {
    if !config.enabled {
        return Err(CliError::AiNotConfigured);
    }

    match config.provider {
        AiProviderType::Openai | AiProviderType::Custom => {
            Ok(Box::new(OpenAiProvider::from_config(config)?))
        }
        AiProviderType::Anthropic => Ok(Box::new(AnthropicProvider::from_config(config)?)),
        AiProviderType::Ollama => Ok(Box::new(OllamaProvider::from_config(config))),
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Prompt building
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Predefined AI task types that map to `AiConfig.tasks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTask {
    Dockerfile,
    Compose,
    Troubleshoot,
    Optimize,
}

impl AiTask {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dockerfile => "dockerfile",
            Self::Compose => "compose",
            Self::Troubleshoot => "troubleshoot",
            Self::Optimize => "optimize",
        }
    }
}

/// Context for building AI prompts.
#[derive(Debug, Clone, Default)]
pub struct PromptContext {
    pub project_type: Option<AppType>,
    pub files: Vec<String>,
    pub error_log: Option<String>,
    pub current_config: Option<String>,
}

/// System message providing context about the stacker CLI.
const SYSTEM_CONTEXT: &str = "\
You are an expert DevOps assistant integrated into the `stacker` CLI tool. \
Stacker helps developers deploy web applications using Docker, docker-compose, \
Terraform, and Ansible. You provide concise, production-ready configurations. \
Always use multi-stage builds when appropriate. Prefer Alpine-based images. \
Include health checks. Follow Docker and security best practices.";

/// Build a prompt for Dockerfile generation.
pub fn build_dockerfile_prompt(ctx: &PromptContext) -> (String, String) {
    let project_type = ctx
        .project_type
        .map(|t| t.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let files_list = if ctx.files.is_empty() {
        "No files detected".to_string()
    } else {
        ctx.files.join(", ")
    };

    let prompt = format!(
        "Generate an optimized Dockerfile for a {} project.\n\
         Detected files: {}\n\
         Requirements:\n\
         - Multi-stage build if applicable\n\
         - Alpine base image preferred\n\
         - Non-root user\n\
         - .dockerignore recommendations\n\
         Return only the Dockerfile content.",
        project_type, files_list
    );

    (SYSTEM_CONTEXT.to_string(), prompt)
}

/// Build a prompt for docker-compose generation/improvement.
pub fn build_compose_prompt(ctx: &PromptContext) -> (String, String) {
    let project_type = ctx
        .project_type
        .map(|t| t.to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let current = ctx
        .current_config
        .as_deref()
        .unwrap_or("No existing compose file");

    let prompt = format!(
        "Generate or improve a docker-compose.yml for a {} project.\n\
         Current config:\n```yaml\n{}\n```\n\
         Requirements:\n\
         - Named volumes for persistence\n\
         - Health checks for services\n\
         - Proper networking\n\
         - Resource limits\n\
         Return only the docker-compose.yml content.",
        project_type, current
    );

    (SYSTEM_CONTEXT.to_string(), prompt)
}

/// Build a prompt for troubleshooting deployment issues.
pub fn build_troubleshoot_prompt(ctx: &PromptContext) -> (String, String) {
    let error = ctx
        .error_log
        .as_deref()
        .unwrap_or("No error log provided");

    let prompt = format!(
        "Diagnose and fix the following deployment issue.\n\
         Error log:\n```\n{}\n```\n\
         Provide:\n\
         1. Root cause analysis\n\
         2. Step-by-step fix\n\
         3. Prevention recommendations",
        error
    );

    (SYSTEM_CONTEXT.to_string(), prompt)
}

/// Build a prompt based on task type.
pub fn build_prompt(task: AiTask, ctx: &PromptContext) -> (String, String) {
    match task {
        AiTask::Dockerfile => build_dockerfile_prompt(ctx),
        AiTask::Compose => build_compose_prompt(ctx),
        AiTask::Troubleshoot => build_troubleshoot_prompt(ctx),
        AiTask::Optimize => build_dockerfile_prompt(ctx), // reuse dockerfile optimization
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    // ── Mock provider ───────────────────────────────

    struct MockAiProvider {
        response: String,
    }

    impl MockAiProvider {
        fn with_response(response: &str) -> Self {
            Self {
                response: response.to_string(),
            }
        }
    }

    impl AiProvider for MockAiProvider {
        fn name(&self) -> &str {
            "mock"
        }

        fn complete(&self, _prompt: &str, _context: &str) -> Result<String, CliError> {
            Ok(self.response.clone())
        }
    }

    // ── Phase 7 tests ───────────────────────────────

    #[test]
    fn test_ai_provider_from_config_openai() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Openai,
            model: Some("gpt-4o".to_string()),
            api_key: Some("sk-test-key".to_string()),
            endpoint: None,
            tasks: vec!["dockerfile".to_string()],
        };

        let provider = create_provider(&config).unwrap();
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_ai_provider_from_config_ollama() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Ollama,
            model: None,
            api_key: None,
            endpoint: Some("http://localhost:11434/api/chat".to_string()),
            tasks: vec![],
        };

        let provider = create_provider(&config).unwrap();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_ai_provider_from_config_anthropic() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Anthropic,
            model: Some("claude-sonnet-4-20250514".to_string()),
            api_key: Some("sk-ant-test".to_string()),
            endpoint: None,
            tasks: vec![],
        };

        let provider = create_provider(&config).unwrap();
        assert_eq!(provider.name(), "anthropic");
    }

    #[test]
    fn test_mock_ai_complete() {
        let provider = MockAiProvider::with_response("Use FROM node:lts-alpine");
        let result = provider.complete("optimize dockerfile", "system context").unwrap();
        assert!(result.contains("node:lts-alpine"));
    }

    #[test]
    fn test_ai_build_prompt_for_dockerfile() {
        let ctx = PromptContext {
            project_type: Some(AppType::Node),
            files: vec!["package.json".to_string(), "src/index.ts".to_string()],
            error_log: None,
            current_config: None,
        };

        let (system, prompt) = build_dockerfile_prompt(&ctx);
        assert!(system.contains("DevOps"));
        assert!(prompt.contains("node"));
        assert!(prompt.contains("Dockerfile"));
        assert!(prompt.contains("package.json"));
    }

    #[test]
    fn test_ai_build_prompt_for_troubleshoot() {
        let ctx = PromptContext {
            project_type: None,
            files: vec![],
            error_log: Some("connection refused on port 5432".to_string()),
            current_config: None,
        };

        let (_, prompt) = build_troubleshoot_prompt(&ctx);
        assert!(prompt.contains("connection refused"));
        assert!(prompt.contains("Diagnose"));
    }

    #[test]
    fn test_ai_not_configured_returns_error() {
        let config = AiConfig {
            enabled: false,
            ..Default::default()
        };

        let result = create_provider(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            CliError::AiNotConfigured => {} // expected
            other => panic!("Expected AiNotConfigured, got: {:?}", other),
        }
    }

    // ── Additional tests ────────────────────────────

    #[test]
    fn test_openai_requires_api_key() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Openai,
            api_key: None,
            ..Default::default()
        };

        let result = create_provider(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_anthropic_requires_api_key() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Anthropic,
            api_key: None,
            ..Default::default()
        };

        let result = create_provider(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_ollama_no_api_key_needed() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Ollama,
            api_key: None,
            ..Default::default()
        };

        let provider = create_provider(&config).unwrap();
        assert_eq!(provider.name(), "ollama");
    }

    #[test]
    fn test_custom_provider_uses_openai_compat() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Custom,
            api_key: Some("custom-key".to_string()),
            endpoint: Some("https://my-llm.local/v1/chat/completions".to_string()),
            model: Some("my-model".to_string()),
            tasks: vec![],
        };

        let provider = create_provider(&config).unwrap();
        // Custom uses OpenAI-compatible protocol
        assert_eq!(provider.name(), "openai");
    }

    #[test]
    fn test_default_models() {
        assert_eq!(default_model(AiProviderType::Openai), "gpt-4o");
        assert_eq!(default_model(AiProviderType::Ollama), "llama3");
        assert!(default_model(AiProviderType::Anthropic).contains("claude"));
    }

    #[test]
    fn test_build_compose_prompt() {
        let ctx = PromptContext {
            project_type: Some(AppType::Python),
            files: vec![],
            error_log: None,
            current_config: Some("version: '3'\nservices:\n  web:\n    image: python:3.11".to_string()),
        };

        let (_, prompt) = build_compose_prompt(&ctx);
        assert!(prompt.contains("python"));
        assert!(prompt.contains("docker-compose.yml"));
        assert!(prompt.contains("python:3.11"));
    }

    #[test]
    fn test_build_prompt_dispatches_correctly() {
        let ctx = PromptContext {
            project_type: Some(AppType::Rust),
            files: vec!["Cargo.toml".to_string()],
            ..Default::default()
        };

        let (_, dockerfile_prompt) = build_prompt(AiTask::Dockerfile, &ctx);
        assert!(dockerfile_prompt.contains("rust"));

        let (_, compose_prompt) = build_prompt(AiTask::Compose, &ctx);
        assert!(compose_prompt.contains("docker-compose"));

        let troubleshoot_ctx = PromptContext {
            error_log: Some("exit code 1".to_string()),
            ..Default::default()
        };
        let (_, troubleshoot_prompt) = build_prompt(AiTask::Troubleshoot, &troubleshoot_ctx);
        assert!(troubleshoot_prompt.contains("exit code 1"));
    }

    #[test]
    fn test_ai_task_as_str() {
        assert_eq!(AiTask::Dockerfile.as_str(), "dockerfile");
        assert_eq!(AiTask::Compose.as_str(), "compose");
        assert_eq!(AiTask::Troubleshoot.as_str(), "troubleshoot");
        assert_eq!(AiTask::Optimize.as_str(), "optimize");
    }

    #[test]
    fn test_openai_provider_from_config_defaults() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Openai,
            api_key: Some("sk-test".to_string()),
            model: None,
            endpoint: None,
            tasks: vec![],
        };

        let provider = OpenAiProvider::from_config(&config).unwrap();
        assert_eq!(provider.endpoint, OPENAI_API_URL);
        assert_eq!(provider.model, "gpt-4o");
    }

    #[test]
    fn test_ollama_provider_from_config_defaults() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Ollama,
            ..Default::default()
        };

        let provider = OllamaProvider::from_config(&config);
        assert_eq!(provider.endpoint, OLLAMA_API_URL);
        // Model is either auto-detected from running Ollama or falls back to default
        assert!(!provider.model.is_empty(), "model must not be empty");
    }

    #[test]
    fn test_ollama_provider_from_config_explicit_model() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Ollama,
            model: Some("custom-model".to_string()),
            ..Default::default()
        };

        let provider = OllamaProvider::from_config(&config);
        assert_eq!(provider.model, "custom-model");
    }

    #[test]
    fn test_prompt_context_default() {
        let ctx = PromptContext::default();
        assert!(ctx.project_type.is_none());
        assert!(ctx.files.is_empty());
        assert!(ctx.error_log.is_none());
        assert!(ctx.current_config.is_none());
    }

    // ── Timeout resolution tests ────────────────

    #[test]
    fn test_resolve_timeout_uses_config_value() {
        // Clean env to avoid interference
        std::env::remove_var("STACKER_AI_TIMEOUT");
        assert_eq!(resolve_timeout(600), 600);
        assert_eq!(resolve_timeout(30), 30);
    }

    #[test]
    fn test_resolve_timeout_default_fallback() {
        std::env::remove_var("STACKER_AI_TIMEOUT");
        // 0 means "use default"
        assert_eq!(resolve_timeout(0), DEFAULT_AI_TIMEOUT_SECS);
    }

    #[test]
    fn test_resolve_timeout_env_overrides_config() {
        std::env::set_var("STACKER_AI_TIMEOUT", "900");
        assert_eq!(resolve_timeout(300), 900);
        std::env::remove_var("STACKER_AI_TIMEOUT");
    }

    #[test]
    fn test_resolve_timeout_env_invalid_ignored() {
        std::env::set_var("STACKER_AI_TIMEOUT", "not-a-number");
        assert_eq!(resolve_timeout(120), 120);
        std::env::remove_var("STACKER_AI_TIMEOUT");
    }

    #[test]
    fn test_provider_timeout_from_config() {
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Ollama,
            timeout: 600,
            ..Default::default()
        };
        let provider = OllamaProvider::from_config(&config);
        assert_eq!(provider.timeout_secs, 600);
    }

    #[test]
    fn test_openai_provider_timeout_from_config() {
        std::env::remove_var("STACKER_AI_TIMEOUT");
        let config = AiConfig {
            enabled: true,
            provider: AiProviderType::Openai,
            api_key: Some("sk-test".to_string()),
            timeout: 120,
            ..Default::default()
        };
        let provider = OpenAiProvider::from_config(&config).unwrap();
        assert_eq!(provider.timeout_secs, 120);
    }
}
