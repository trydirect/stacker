//! Standalone `stacker` CLI binary.
//!
//! Exposes the Stacker CLI commands directly at the top level:
//!
//! ```text
//! stacker init
//! stacker deploy --target local
//! stacker status
//! stacker logs --follow
//! stacker destroy --confirm
//! ```
//!
//! Unlike the `console` binary (which nests these under `stacker` subcommand
//! alongside other admin tools), this binary is a lightweight entry point
//! designed for end-user distribution.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "stacker",
    version,
    about = "Deploy apps from a stacker.yml config",
    long_about = "Stacker CLI — build, deploy, and manage containerised applications\n\n\
        Create a stacker.yml configuration file, and Stacker will generate\n\
        Dockerfiles, docker-compose definitions, and deploy your stack locally\n\
        or to cloud providers with a single command."
)]
struct Cli {
    #[command(subcommand)]
    command: StackerCommands,
}

#[derive(Debug, Subcommand)]
enum StackerCommands {
    /// Authenticate with the TryDirect platform
    Login {
        /// Organisation slug (for multi-org accounts)
        #[arg(long)]
        org: Option<String>,
        /// Custom platform domain
        #[arg(long)]
        domain: Option<String>,
        /// API base URL (default: https://try.direct)
        #[arg(long)]
        api_url: Option<String>,
    },
    /// Initialize a new stacker project (generates stacker.yml + Dockerfile)
    Init {
        /// Application type: static, node, python, rust, go, php
        #[arg(long, value_name = "TYPE")]
        app_type: Option<String>,
        /// Include reverse-proxy configuration
        #[arg(long)]
        with_proxy: bool,
        /// Use AI to scan the project and generate a tailored stacker.yml
        #[arg(long)]
        with_ai: bool,
        /// AI provider: openai, anthropic, ollama, custom (default: ollama)
        #[arg(long, value_name = "PROVIDER")]
        ai_provider: Option<String>,
        /// AI model name (e.g. gpt-4o, claude-sonnet-4-20250514, llama3)
        #[arg(long, value_name = "MODEL")]
        ai_model: Option<String>,
        /// AI API key (or set OPENAI_API_KEY / ANTHROPIC_API_KEY env var)
        #[arg(long, value_name = "KEY")]
        ai_api_key: Option<String>,
    },
    /// Build & deploy the stack
    Deploy {
        /// Deployment target: local, cloud, server
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Show what would be deployed without executing
        #[arg(long)]
        dry_run: bool,
        /// Force rebuild of all containers
        #[arg(long)]
        force_rebuild: bool,
    },
    /// Show container logs
    Logs {
        /// Show logs for a specific service only
        #[arg(long)]
        service: Option<String>,
        /// Follow log output (stream)
        #[arg(long, short)]
        follow: bool,
        /// Number of lines to show from the end
        #[arg(long)]
        tail: Option<u32>,
        /// Show logs since timestamp (e.g. "2h", "2024-01-01")
        #[arg(long)]
        since: Option<String>,
    },
    /// Show deployment status
    Status {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Watch for changes (refresh periodically)
        #[arg(long)]
        watch: bool,
    },
    /// Tear down the deployed stack
    Destroy {
        /// Also remove named volumes
        #[arg(long)]
        volumes: bool,
        /// Skip confirmation prompt (required)
        #[arg(long, short = 'y')]
        confirm: bool,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
    /// AI-assisted operations
    Ai {
        #[command(subcommand)]
        command: AiCommands,
    },
    /// Reverse-proxy management
    Proxy {
        #[command(subcommand)]
        command: ProxyCommands,
    },
    /// Check for updates and self-update
    Update {
        /// Release channel: stable, beta
        #[arg(long)]
        channel: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigCommands {
    /// Validate stacker.yml syntax and semantics
    Validate {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Show resolved configuration
    Show {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum AiCommands {
    /// Ask the AI a question about your stack
    Ask {
        /// The question to ask
        question: String,
        /// Path to a file to include as context
        #[arg(long)]
        context: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ProxyCommands {
    /// Add a reverse-proxy entry for a domain
    Add {
        /// Domain name (e.g. example.com)
        domain: String,
        /// Upstream service address (e.g. http://app:8080)
        #[arg(long)]
        upstream: Option<String>,
        /// SSL mode: auto, manual, off
        #[arg(long)]
        ssl: Option<String>,
    },
    /// Detect existing reverse-proxy containers
    Detect,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    get_command(cli)?.call()
}

fn get_command(
    cli: Cli,
) -> Result<Box<dyn stacker::console::commands::CallableTrait>, Box<dyn std::error::Error>> {
    let cmd: Box<dyn stacker::console::commands::CallableTrait> = match cli.command {
        StackerCommands::Login {
            org,
            domain,
            api_url,
        } => Box::new(
            stacker::console::commands::cli::login::LoginCommand::new(org, domain, api_url),
        ),
        StackerCommands::Init {
            app_type,
            with_proxy,
            with_ai,
            ai_provider,
            ai_model,
            ai_api_key,
        } => Box::new(
            stacker::console::commands::cli::init::InitCommand::new(
                app_type, with_proxy, with_ai,
            )
            .with_ai_options(ai_provider, ai_model, ai_api_key),
        ),
        StackerCommands::Deploy {
            target,
            file,
            dry_run,
            force_rebuild,
        } => Box::new(
            stacker::console::commands::cli::deploy::DeployCommand::new(
                target,
                file,
                dry_run,
                force_rebuild,
            ),
        ),
        StackerCommands::Logs {
            service,
            follow,
            tail,
            since,
        } => Box::new(stacker::console::commands::cli::logs::LogsCommand::new(
            service, follow, tail, since,
        )),
        StackerCommands::Status { json, watch } => Box::new(
            stacker::console::commands::cli::status::StatusCommand::new(json, watch),
        ),
        StackerCommands::Destroy { volumes, confirm } => Box::new(
            stacker::console::commands::cli::destroy::DestroyCommand::new(volumes, confirm),
        ),
        StackerCommands::Config { command: cfg_cmd } => match cfg_cmd {
            ConfigCommands::Validate { file } => Box::new(
                stacker::console::commands::cli::config::ConfigValidateCommand::new(file),
            ),
            ConfigCommands::Show { file } => Box::new(
                stacker::console::commands::cli::config::ConfigShowCommand::new(file),
            ),
        },
        StackerCommands::Ai { command: ai_cmd } => match ai_cmd {
            AiCommands::Ask { question, context } => Box::new(
                stacker::console::commands::cli::ai::AiAskCommand::new(question, context),
            ),
        },
        StackerCommands::Proxy {
            command: proxy_cmd,
        } => match proxy_cmd {
            ProxyCommands::Add {
                domain,
                upstream,
                ssl,
            } => Box::new(
                stacker::console::commands::cli::proxy::ProxyAddCommand::new(
                    domain, upstream, ssl,
                ),
            ),
            ProxyCommands::Detect => Box::new(
                stacker::console::commands::cli::proxy::ProxyDetectCommand::new(),
            ),
        },
        StackerCommands::Update { channel } => Box::new(
            stacker::console::commands::cli::update::UpdateCommand::new(channel),
        ),
    };

    Ok(cmd)
}
