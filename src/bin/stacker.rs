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

use clap::{Args, CommandFactory, Parser, Subcommand};

fn print_banner() {
    let version = env!("CARGO_PKG_VERSION");
    println!("============================================================");
    println!("stacker-cli v{}", version);
    println!("Stacker CLI - build, deploy, and manage application stacks");
    println!("============================================================");
    println!();
    println!("Getting started:");
    println!("  1) stacker-cli stacker login");
    println!("  2) stacker-cli stacker init --with-cloud");
    println!("  3) stacker-cli stacker deploy --target cloud");
    println!("  4) stacker-cli stacker status --watch");
    println!();
    println!("Run `stacker-cli --help` to see all commands and options.");
    println!();
}

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
        /// ID of saved cloud credential to reuse (from `stacker list clouds`)
        #[arg(long, value_name = "CLOUD_ID")]
        key_id: Option<i32>,
        /// Name of saved server to reuse (overrides deploy.cloud.server in stacker.yml)
        #[arg(long, value_name = "SERVER_NAME")]
        server: Option<String>,
        /// Watch deployment progress until complete (default for cloud deploys)
        #[arg(long)]
        watch: bool,
        /// Disable automatic progress watching after deploy
        #[arg(long)]
        no_watch: bool,
        /// Persist server details into stacker.yml after deploy (for redeploy)
        #[arg(long)]
        lock: bool,
        /// Skip server pre-check; force fresh cloud provision even if deploy.server exists
        #[arg(long)]
        force_new: bool,
    },
    /// Submit current stack to the marketplace for review
    Submit {
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Stack version (default: from stacker.yml or "1.0.0")
        #[arg(long)]
        version: Option<String>,
        /// Short description for marketplace listing
        #[arg(long)]
        description: Option<String>,
        /// Category code (e.g. ai-agents, data-pipelines, saas-starter)
        #[arg(long)]
        category: Option<String>,
        /// Pricing: free, one_time, subscription (default: free)
        #[arg(long, value_name = "TYPE")]
        plan_type: Option<String>,
        /// Price amount (required if plan_type is not free)
        #[arg(long)]
        price: Option<f64>,
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
    /// Force-complete a stuck (paused/error) deployment
    Resolve {
        /// Skip confirmation prompt (required)
        #[arg(long, short = 'y')]
        confirm: bool,
    },
    /// Check for updates and self-update
    Update {
        /// Release channel: stable, beta
        #[arg(long)]
        channel: Option<String>,
    },
    /// Generate shell completion scripts
    Completion {
        /// Shell: bash, zsh, fish, elvish, powershell
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Manage secrets and environment variables in .env
    Secrets {
        #[command(subcommand)]
        command: SecretsCommands,
    },
    /// CI/CD pipeline export and validation
    Ci {
        #[command(subcommand)]
        command: CiCommands,
    },
    /// Status Panel agent control (health, logs, restart, deploy)
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// Marketplace operations (submit, check status)
    Marketplace {
        #[command(subcommand)]
        command: MarketplaceCommands,
    },
}

#[derive(Debug, Subcommand)]
enum MarketplaceCommands {
    /// Check submission status for your marketplace templates
    Status {
        /// Stack name to check (omit for all)
        name: Option<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
    /// Show review comments and history for a submission
    Logs {
        /// Stack name
        name: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
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
    /// List deployments
    Deployments {
        /// Filter by project ID
        #[arg(long)]
        project: Option<i32>,
        /// Limit number of results
        #[arg(long)]
        limit: Option<i64>,
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
    /// List saved cloud credentials
    Clouds {
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
    /// Inject the Vault-stored public key into a server's authorized_keys via a working local key
    Inject {
        /// Server ID whose Vault public key should be injected
        #[arg(long)]
        server_id: i32,
        /// Path to a local private key that already grants SSH access to the server
        #[arg(long, value_name = "FILE")]
        with_key: std::path::PathBuf,
        /// SSH user on the remote server (default: root)
        #[arg(long)]
        user: Option<String>,
        /// SSH port override (default: server's stored port or 22)
        #[arg(long)]
        port: Option<u16>,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommands {
    /// Add a service from the template catalog to stacker.yml (interactive picker when no name given)
    Add {
        /// Service name (e.g. postgres, redis, wordpress, mysql) — omit for interactive picker
        name: Option<String>,
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Remove a service from stacker.yml
    Remove {
        /// Service name to remove
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
    /// Persist deployment lock into stacker.yml (writes deploy.server from last deploy)
    Lock {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Remove deploy.server section from stacker.yml (allows fresh cloud provision)
    Unlock {
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
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

#[derive(Debug, Subcommand)]
enum SecretsCommands {
    /// Set or update a secret in the .env file
    Set {
        /// KEY=VALUE pair (e.g. DB_PASS=s3cr3t)
        key_value: String,
        /// Path to .env file (default: from stacker.yml env_file, or .env)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Get a secret value from the .env file
    Get {
        /// Key name to retrieve
        key: String,
        /// Path to .env file
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Show the actual value instead of masking it
        #[arg(long)]
        show: bool,
    },
    /// List all secrets in the .env file
    List {
        /// Path to .env file
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Show actual values (default: mask with ***)
        #[arg(long)]
        show: bool,
    },
    /// Delete a secret from the .env file
    Delete {
        /// Key name to delete
        key: String,
        /// Path to .env file
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Validate all ${VAR} references in stacker.yml are set in .env or environment
    Validate {
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum CiCommands {
    /// Export a CI/CD pipeline configuration file
    Export {
        /// Platform: github, gitlab
        #[arg(long)]
        platform: String,
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
    },
    /// Validate that the CI/CD pipeline is in sync with stacker.yml
    Validate {
        /// Platform: github, gitlab
        #[arg(long)]
        platform: String,
    },
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    /// Check container health on the remote deployment
    Health {
        /// App code to check (default: all containers)
        #[arg(long)]
        app: Option<String>,
        /// Include system containers (status_panel, compose-agent)
        #[arg(long)]
        system: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash (auto-detected from lock/config)
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Fetch container logs from the remote deployment
    Logs {
        /// App code to fetch logs for (default: statuspanel + statuspanel_agent)
        app: Option<String>,
        /// Maximum number of log lines
        #[arg(long, default_value_t = 400)]
        limit: i32,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Restart a container on the remote deployment
    Restart {
        /// App code to restart
        app: String,
        /// Force restart (stop + start instead of graceful restart)
        #[arg(long)]
        force: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Deploy/update an app container on the remote deployment
    #[command(name = "deploy-app")]
    DeployApp {
        /// App code to deploy
        app: String,
        /// Docker image to use (overrides compose config)
        #[arg(long)]
        image: Option<String>,
        /// Force recreate the container
        #[arg(long)]
        force: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Remove an app container from the remote deployment
    #[command(name = "remove-app")]
    RemoveApp {
        /// App code to remove
        app: String,
        /// Also remove volumes
        #[arg(long)]
        volumes: bool,
        /// Also remove the image
        #[arg(long)]
        remove_image: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Configure iptables firewall rules on the remote deployment
    #[command(name = "configure-firewall")]
    ConfigureFirewall {
        /// Action: add, remove, list, flush
        #[arg(long, default_value = "add")]
        action: String,
        /// List current firewall rules (shortcut for --action list)
        #[arg(long)]
        list: bool,
        /// App code for context/logging
        #[arg(long)]
        app: Option<String>,
        /// Public ports (open to all), comma-separated: "80/tcp,443/tcp,53/udp"
        #[arg(long, value_delimiter = ',')]
        public_ports: Vec<String>,
        /// Private ports (restricted), format: "port/proto:source", comma-separated: "5432/tcp:10.0.0.0/8"
        #[arg(long, value_delimiter = ',')]
        private_ports: Vec<String>,
        /// Persist rules across reboots
        #[arg(long, default_value_t = true)]
        persist: bool,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Configure reverse proxy for an app
    #[command(name = "configure-proxy")]
    ConfigureProxy {
        /// App code
        app: String,
        /// Domain name
        #[arg(long)]
        domain: String,
        /// Port to forward to
        #[arg(long)]
        port: u16,
        /// Enable SSL (default: true)
        #[arg(long, default_value_t = true)]
        ssl: bool,
        /// Action: create, update, delete
        #[arg(long, default_value = "create")]
        action: String,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// List deployment resources from the agent
    #[command(name = "list")]
    List {
        #[command(subcommand)]
        command: AgentListCommands,
    },
    /// Show agent and container status for the deployment
    Status {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Show command history for the deployment
    History {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Send a raw command to the agent (advanced)
    Exec {
        /// Command type (e.g. health, logs, restart, deploy_app, etc.)
        command_type: String,
        /// JSON parameters
        #[arg(long)]
        params: Option<String>,
        /// Timeout in seconds
        #[arg(long)]
        timeout: Option<u64>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// Install the Status Panel agent on an existing deployed server
    Install {
        /// Path to stacker.yml (default: ./stacker.yml)
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AgentListCommands {
    /// List apps deployed for the target deployment
    Apps {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
    },
    /// List containers running on the target server
    Containers {
        /// Output in JSON format
        #[arg(long)]
        json: bool,
        /// Deployment hash
        #[arg(long)]
        deployment: Option<String>,
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
    Detect {
        /// Output as JSON
        #[arg(long)]
        json: bool,
        /// Target a specific deployment by hash
        #[arg(long)]
        deployment: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            use clap::error::ErrorKind;
            match err.kind() {
                ErrorKind::DisplayHelp => {
                    print_banner();
                    err.print()?;
                    return Ok(());
                }
                ErrorKind::DisplayVersion => {
                    err.print()?;
                    return Ok(());
                }
                _ => {
                    err.print()?;
                    std::process::exit(2);
                }
            }
        }
    };

    let Some(subcommand) = cli.command else {
        print_banner();
        let mut cmd = Cli::command();
        cmd.print_long_help()?;
        println!();
        return Ok(());
    };

    // Shell completions need access to the CLI Command object directly.
    if let StackerCommands::Completion { shell } = subcommand {
        let mut cmd = Cli::command();
        clap_complete::generate(shell, &mut cmd, "stacker", &mut std::io::stdout());
        eprintln!();
        eprintln!("# Reload your shell or run:  source ~/.zshrc  (for zsh)");
        return Ok(());
    }

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
            key_id,
            server,
            watch,
            no_watch,
            lock,
            force_new,
        } => Box::new(
            stacker::console::commands::cli::deploy::DeployCommand::new(
                target,
                file,
                dry_run,
                force_rebuild,
            )
            .with_remote_overrides(project, key, server)
            .with_key_id(key_id)
            .with_watch(watch, no_watch)
            .with_lock(lock)
            .with_force_new(force_new),
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
            ConfigCommands::Lock { file } => Box::new(
                stacker::console::commands::cli::config::ConfigLockCommand::new(file),
            ),
            ConfigCommands::Unlock { file } => Box::new(
                stacker::console::commands::cli::config::ConfigUnlockCommand::new(file),
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
            ProxyCommands::Detect { json, deployment } => Box::new(
                stacker::console::commands::cli::proxy::ProxyDetectCommand::new(json, deployment),
            ),
        },
        StackerCommands::List { command: list_cmd } => match list_cmd {
            ListCommands::Projects { json } => Box::new(
                stacker::console::commands::cli::list::ListProjectsCommand::new(json),
            ),
            ListCommands::Deployments { json, project, limit } => Box::new(
                stacker::console::commands::cli::list::ListDeploymentsCommand::new(
                    json, project, limit,
                ),
            ),
            ListCommands::Servers { json } => Box::new(
                stacker::console::commands::cli::list::ListServersCommand::new(json),
            ),
            ListCommands::SshKeys { json } => Box::new(
                stacker::console::commands::cli::list::ListSshKeysCommand::new(json),
            ),
            ListCommands::Clouds { json } => Box::new(
                stacker::console::commands::cli::list::ListCloudsCommand::new(json),
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
            SshKeyCommands::Inject {
                server_id,
                with_key,
                user,
                port,
            } => Box::new(
                stacker::console::commands::cli::ssh_key::SshKeyInjectCommand::new(
                    server_id, with_key, user, port,
                ),
            ),
        },
        StackerCommands::Service { command: svc_cmd } => match svc_cmd {
            ServiceCommands::Add { name, file } => Box::new(
                stacker::console::commands::cli::service::ServiceAddCommand::new(name, file),
            ),
            ServiceCommands::Remove { name, file } => Box::new(
                stacker::console::commands::cli::service::ServiceRemoveCommand::new(name, file),
            ),
            ServiceCommands::List { online } => Box::new(
                stacker::console::commands::cli::service::ServiceListCommand::new(online),
            ),
        },
        StackerCommands::Resolve { confirm } => Box::new(
            stacker::console::commands::cli::resolve::ResolveCommand::new(confirm),
        ),
        StackerCommands::Update { channel } => Box::new(
            stacker::console::commands::cli::update::UpdateCommand::new(channel),
        ),
        StackerCommands::Secrets { command: sec_cmd } => match sec_cmd {
            SecretsCommands::Set { key_value, file } => Box::new(
                stacker::console::commands::cli::secrets::SecretsSetCommand::new(key_value, file),
            ),
            SecretsCommands::Get { key, file, show } => Box::new(
                stacker::console::commands::cli::secrets::SecretsGetCommand::new(key, file, show),
            ),
            SecretsCommands::List { file, show } => Box::new(
                stacker::console::commands::cli::secrets::SecretsListCommand::new(file, show),
            ),
            SecretsCommands::Delete { key, file } => Box::new(
                stacker::console::commands::cli::secrets::SecretsDeleteCommand::new(key, file),
            ),
            SecretsCommands::Validate { file } => Box::new(
                stacker::console::commands::cli::secrets::SecretsValidateCommand::new(file),
            ),
        },
        StackerCommands::Ci { command: ci_cmd } => match ci_cmd {
            CiCommands::Export { platform, file } => Box::new(
                stacker::console::commands::cli::ci::CiExportCommand::new(platform, file),
            ),
            CiCommands::Validate { platform } => Box::new(
                stacker::console::commands::cli::ci::CiValidateCommand::new(platform),
            ),
        },
        StackerCommands::Agent { command: agent_cmd } => {
            use stacker::console::commands::cli::agent;
            match agent_cmd {
                AgentCommands::Health { app, system, json, deployment } => Box::new(
                    agent::AgentHealthCommand::new(app, json, deployment, system),
                ),
                AgentCommands::Logs { app, limit, json, deployment } => Box::new(
                    agent::AgentLogsCommand::new(app, Some(limit), json, deployment),
                ),
                AgentCommands::Restart { app, force, json, deployment } => Box::new(
                    agent::AgentRestartCommand::new(app, force, json, deployment),
                ),
                AgentCommands::DeployApp { app, image, force, json, deployment } => Box::new(
                    agent::AgentDeployAppCommand::new(app, image, force, json, deployment),
                ),
                AgentCommands::RemoveApp { app, volumes, remove_image, json, deployment } => Box::new(
                    agent::AgentRemoveAppCommand::new(app, volumes, remove_image, json, deployment),
                ),
                AgentCommands::ConfigureFirewall { action, list, app, public_ports, private_ports, persist, json, deployment } => {
                    let effective_action = if list { "list".to_string() } else { action };
                    Box::new(agent::AgentConfigureFirewallCommand::new(
                        effective_action,
                        app,
                        public_ports,
                        private_ports,
                        persist,
                        json,
                        deployment,
                    ))
                }
                AgentCommands::ConfigureProxy { app, domain, port, ssl, action, json, deployment } => Box::new(
                    agent::AgentConfigureProxyCommand::new(app, domain, port, ssl, action, json, deployment),
                ),
                AgentCommands::List { command: list_cmd } => match list_cmd {
                    AgentListCommands::Apps { json, deployment } => Box::new(
                        agent::AgentListAppsCommand::new(json, deployment),
                    ),
                    AgentListCommands::Containers { json, deployment } => Box::new(
                        agent::AgentListContainersCommand::new(json, deployment),
                    ),
                },
                AgentCommands::Status { json, deployment } => Box::new(
                    agent::AgentStatusCommand::new(json, deployment),
                ),
                AgentCommands::History { json, deployment } => Box::new(
                    agent::AgentHistoryCommand::new(json, deployment),
                ),
                AgentCommands::Exec { command_type, params, timeout, json, deployment } => Box::new(
                    agent::AgentExecCommand::new(command_type, params, timeout, json, deployment),
                ),
                AgentCommands::Install { file, json } => Box::new(
                    agent::AgentInstallCommand::new(file, json),
                ),
            }
        },
        StackerCommands::Submit {
            file,
            version,
            description,
            category,
            plan_type,
            price,
        } => Box::new(
            stacker::console::commands::cli::submit::SubmitCommand::new(
                file, version, description, category, plan_type, price,
            ),
        ),
        StackerCommands::Marketplace { command: mkt_cmd } => match mkt_cmd {
            MarketplaceCommands::Status { name, json } => Box::new(
                stacker::console::commands::cli::marketplace::MarketplaceStatusCommand::new(
                    name, json,
                ),
            ),
            MarketplaceCommands::Logs { name, json } => Box::new(
                stacker::console::commands::cli::marketplace::MarketplaceLogsCommand::new(
                    name, json,
                ),
            ),
        },
        // Completion is handled in main() before this function is called.
        StackerCommands::Completion { .. } => unreachable!(),
    };

    Ok(cmd)
}
