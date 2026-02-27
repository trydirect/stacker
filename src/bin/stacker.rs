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

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "stacker",
    version,
    about = "Deploy apps from a stacker.yml config",
    long_about = "Stacker CLI — build, deploy, and manage containerised applications\n\n\
        Create a stacker.yml configuration file, and Stacker will generate\n\
        Dockerfiles, docker-compose definitions, and deploy your stack locally\n\
        or to cloud providers with a single command.",
    subcommand_required = false,
    arg_required_else_help = false
)]
struct Cli {
    #[command(subcommand)]
    command: Option<StackerCommands>,
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
        /// API base URL (default: https://api.try.direct)
        #[arg(long = "auth-url", visible_alias = "api-url")]
        auth_url: Option<String>,
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
        /// Immediately run cloud setup wizard after init
        #[arg(long)]
        with_cloud: bool,
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
        /// Project name on the Stacker server (overrides project.identity in stacker.yml)
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
        /// Name of saved cloud credential to reuse (overrides deploy.cloud.key in stacker.yml)
        #[arg(long, value_name = "KEY_NAME")]
        key: Option<String>,
        /// Name of saved server to reuse (overrides deploy.cloud.server in stacker.yml)
        #[arg(long, value_name = "SERVER_NAME")]
        server: Option<String>,
        /// Watch deployment progress until complete (default for cloud deploys)
        #[arg(long)]
        watch: bool,
        /// Disable automatic progress watching after deploy
        #[arg(long)]
        no_watch: bool,
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
    /// AI-assisted operations — run `stacker ai` for interactive chat
    Ai(AiArgs),
    /// Reverse-proxy management
    Proxy {
        #[command(subcommand)]
        command: ProxyCommands,
    },
    /// List resources (projects, servers, ssh-keys)
    List {
        #[command(subcommand)]
        command: ListCommands,
    },
    /// SSH key management (generate, show, upload)
    #[command(name = "ssh-key")]
    SshKey {
        #[command(subcommand)]
        command: SshKeyCommands,
    },
    /// Service template management (add services to stacker.yml)
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    /// Check for updates and self-update
    Update {
        /// Release channel: stable, beta
        #[arg(long)]
        channel: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ListCommands {
    /// List all projects
    Projects {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// List all servers
    Servers {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// List SSH keys (per-server key status)
    #[command(name = "ssh-keys")]
    SshKeys {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SshKeyCommands {
    /// Generate a new SSH key pair for a server (stored in Vault)
    Generate {
        /// Server ID to generate the key for
        #[arg(long)]
        server_id: i32,
        /// Save private key to this file (if Vault storage fails)
        #[arg(long, value_name = "PATH")]
        save_to: Option<std::path::PathBuf>,
    },
    /// Show the public SSH key for a server
    Show {
        /// Server ID to show the key for
        #[arg(long)]
        server_id: i32,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Upload an existing SSH key pair for a server
    Upload {
        /// Server ID to upload the key for
        #[arg(long)]
        server_id: i32,
        /// Path to public key file
        #[arg(long, value_name = "FILE")]
        public_key: std::path::PathBuf,
        /// Path to private key file
        #[arg(long, value_name = "FILE")]
        private_key: std::path::PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommands {
    /// Add a service from the template catalog to stacker.yml
    Add {
        /// Service name (e.g. postgres, redis, wordpress, mysql)
        name: String,
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// List available service templates
    List {
        /// Also query the marketplace API for online templates
        #[arg(long)]
        online: bool,
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
    /// Print a full commented `stacker.yml` reference example
    Example,
    /// Interactively fix missing required config fields
    Fix {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Enable interactive prompts (default: true)
        #[arg(long, default_value_t = true)]
        interactive: bool,
    },
    /// Guided setup helpers
    Setup {
        #[command(subcommand)]
        command: ConfigSetupCommands,
    },
}

#[derive(Debug, Subcommand)]
enum ConfigSetupCommands {
    /// Configure cloud deployment defaults in stacker.yml
    Cloud {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Advanced/debug: generate remote orchestrator payload and wire stacker.yml
    RemotePayload {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        #[arg(long, value_name = "OUT")]
        out: Option<String>,
    },
}

/// Arguments for `stacker ai`.
/// Using a separate struct lets `subcommand_required = false` work so
/// bare `stacker ai` launches the interactive chat mode.
#[derive(Debug, Args)]
#[command(subcommand_required = false, arg_required_else_help = false)]
struct AiArgs {
    #[command(subcommand)]
    command: Option<AiCommands>,
    /// Write mode: AI may create/edit `stacker.yml` and files under `.stacker/`.
    /// Requires a tool-capable model (Ollama: llama3.1/qwen2.5-coder, OpenAI: any).
    #[arg(long)]
    write: bool,
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
        /// Interactively configure AI in stacker.yml before asking
        #[arg(long)]
        configure: bool,
        /// Write mode: AI may create/edit `stacker.yml` and files under `.stacker/`
        #[arg(long)]
        write: bool,
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

    let Some(subcommand) = cli.command else {
        println!("stacker-cli {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    };

    let command = get_command(subcommand)?;
    if let Err(err) = command.call() {
        eprintln!("Error: {}", err);
        std::process::exit(1);
    }
    Ok(())
}

fn get_command(
    subcommand: StackerCommands,
) -> Result<Box<dyn stacker::console::commands::CallableTrait>, Box<dyn std::error::Error>> {
    let cmd: Box<dyn stacker::console::commands::CallableTrait> = match subcommand {
        StackerCommands::Login {
            org,
            domain,
            auth_url,
        } => Box::new(
            stacker::console::commands::cli::login::LoginCommand::new(org, domain, auth_url),
        ),
        StackerCommands::Init {
            app_type,
            with_proxy,
            with_ai,
            with_cloud,
            ai_provider,
            ai_model,
            ai_api_key,
        } => Box::new(
            stacker::console::commands::cli::init::InitCommand::new(
                app_type, with_proxy, with_ai, with_cloud,
            )
            .with_ai_options(ai_provider, ai_model, ai_api_key),
        ),
        StackerCommands::Deploy {
            target,
            file,
            dry_run,
            force_rebuild,
            project,
            key,
            server,
            watch,
            no_watch,
        } => Box::new(
            stacker::console::commands::cli::deploy::DeployCommand::new(
                target,
                file,
                dry_run,
                force_rebuild,
            )
            .with_remote_overrides(project, key, server)
            .with_watch(watch, no_watch),
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
            ConfigCommands::Example => Box::new(
                stacker::console::commands::cli::config::ConfigExampleCommand::new(),
            ),
            ConfigCommands::Fix { file, interactive } => Box::new(
                stacker::console::commands::cli::config::ConfigFixCommand::new(file, interactive),
            ),
            ConfigCommands::Setup { command } => match command {
                ConfigSetupCommands::Cloud { file } => Box::new(
                    stacker::console::commands::cli::config::ConfigSetupCloudCommand::new(file),
                ),
                ConfigSetupCommands::RemotePayload { file, out } => Box::new(
                    stacker::console::commands::cli::config::ConfigSetupRemotePayloadCommand::new(file, out),
                ),
            },
        },
        StackerCommands::Ai(ai_args) => match ai_args.command {
            None => Box::new(
                stacker::console::commands::cli::ai::AiChatCommand::new(ai_args.write),
            ),
            Some(AiCommands::Ask {
                question,
                context,
                configure,
                write,
            }) => Box::new(
                stacker::console::commands::cli::ai::AiAskCommand::new(question, context)
                    .with_configure(configure)
                    .with_write(ai_args.write || write),
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
        StackerCommands::List { command: list_cmd } => match list_cmd {
            ListCommands::Projects { json } => Box::new(
                stacker::console::commands::cli::list::ListProjectsCommand::new(json),
            ),
            ListCommands::Servers { json } => Box::new(
                stacker::console::commands::cli::list::ListServersCommand::new(json),
            ),
            ListCommands::SshKeys { json } => Box::new(
                stacker::console::commands::cli::list::ListSshKeysCommand::new(json),
            ),
        },
        StackerCommands::SshKey { command: ssh_cmd } => match ssh_cmd {
            SshKeyCommands::Generate { server_id, save_to } => Box::new(
                stacker::console::commands::cli::ssh_key::SshKeyGenerateCommand::new(
                    server_id, save_to,
                ),
            ),
            SshKeyCommands::Show { server_id, json } => Box::new(
                stacker::console::commands::cli::ssh_key::SshKeyShowCommand::new(server_id, json),
            ),
            SshKeyCommands::Upload {
                server_id,
                public_key,
                private_key,
            } => Box::new(
                stacker::console::commands::cli::ssh_key::SshKeyUploadCommand::new(
                    server_id, public_key, private_key,
                ),
            ),
        },
        StackerCommands::Service { command: svc_cmd } => match svc_cmd {
            ServiceCommands::Add { name, file } => Box::new(
                stacker::console::commands::cli::service::ServiceAddCommand::new(name, file),
            ),
            ServiceCommands::List { online } => Box::new(
                stacker::console::commands::cli::service::ServiceListCommand::new(online),
            ),
        },
        StackerCommands::Update { channel } => Box::new(
            stacker::console::commands::cli::update::UpdateCommand::new(channel),
        ),
    };

    Ok(cmd)
}
