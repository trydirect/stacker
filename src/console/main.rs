use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    AppClient {
        #[command(subcommand)]
        command: AppClientCommands,
    },
    Debug {
        #[command(subcommand)]
        command: DebugCommands,
    },
    MQ {
        #[command(subcommand)]
        command: AppMqCommands,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// Stacker CLI — deploy apps from a stacker.yml config
    Stacker {
        #[command(subcommand)]
        command: StackerCommands,
    },
}

#[derive(Debug, Subcommand)]
enum AgentCommands {
    RotateToken {
        #[arg(long)]
        deployment_hash: String,
        #[arg(long)]
        new_token: String,
    },
}

#[derive(Debug, Subcommand)]
enum AppClientCommands {
    New {
        #[arg(long)]
        user_id: i32,
    },
}

#[derive(Debug, Subcommand)]
enum DebugCommands {
    Json {
        #[arg(long)]
        line: usize,
        #[arg(long)]
        column: usize,
        #[arg(long)]
        payload: String,
    },
    Casbin {
        #[arg(long)]
        action: String,
        #[arg(long)]
        path: String,
        #[arg(long)]
        subject: String,
    },
    Dockerhub {
        #[arg(long)]
        json: String,
    },
}

#[derive(Debug, Subcommand)]
enum AppMqCommands {
    Listen {},
}

#[derive(Debug, Subcommand)]
enum StackerCommands {
    /// Authenticate with the TryDirect platform
    Login {
        #[arg(long)]
        org: Option<String>,
        #[arg(long)]
        domain: Option<String>,
        /// API base URL (default: https://api.try.direct)
        #[arg(long = "auth-url", visible_alias = "api-url")]
        auth_url: Option<String>,
    },
    /// Initialize a new stacker project (stacker.yml + Dockerfile)
    Init {
        #[arg(long, value_name = "TYPE")]
        app_type: Option<String>,
        #[arg(long)]
        with_proxy: bool,
        #[arg(long)]
        with_ai: bool,
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
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force_rebuild: bool,
        /// Project name on the Stacker server
        #[arg(long, value_name = "NAME")]
        project: Option<String>,
        /// Name of saved cloud credential to reuse
        #[arg(long, value_name = "KEY_NAME")]
        key: Option<String>,
        /// Name of saved server to reuse
        #[arg(long, value_name = "SERVER_NAME")]
        server: Option<String>,
    },
    /// Show container logs
    Logs {
        #[arg(long)]
        service: Option<String>,
        #[arg(long, short)]
        follow: bool,
        #[arg(long)]
        tail: Option<u32>,
        #[arg(long)]
        since: Option<String>,
    },
    /// Show deployment status
    Status {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        watch: bool,
    },
    /// Tear down the deployed stack
    Destroy {
        #[arg(long)]
        volumes: bool,
        #[arg(long, short = 'y')]
        confirm: bool,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        command: StackerConfigCommands,
    },
    /// AI-assisted operations
    Ai {
        #[command(subcommand)]
        command: StackerAiCommands,
    },
    /// Reverse-proxy management
    Proxy {
        #[command(subcommand)]
        command: StackerProxyCommands,
    },
    /// Self-update
    Update {
        #[arg(long)]
        channel: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum StackerConfigCommands {
    /// Validate stacker.yml
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
        #[arg(long, default_value_t = true)]
        interactive: bool,
    },
    /// Guided setup helpers
    Setup {
        #[command(subcommand)]
        command: StackerConfigSetupCommands,
    },
}

#[derive(Debug, Subcommand)]
enum StackerConfigSetupCommands {
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
enum StackerAiCommands {
    /// Ask the AI a question about your stack
    Ask {
        question: String,
        #[arg(long)]
        context: Option<String>,
        #[arg(long)]
        configure: bool,
    },
}

#[derive(Debug, Subcommand)]
enum StackerProxyCommands {
    /// Add a reverse-proxy entry for a domain
    Add {
        domain: String,
        #[arg(long)]
        upstream: Option<String>,
        #[arg(long)]
        ssl: Option<String>,
    },
    /// Detect existing proxy containers
    Detect,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    get_command(cli)?.call()
}

fn get_command(cli: Cli) -> Result<Box<dyn stacker::console::commands::CallableTrait>, String> {
    match cli.command {
        Commands::AppClient { command } => match command {
            AppClientCommands::New { user_id } => Ok(Box::new(
                stacker::console::commands::appclient::NewCommand::new(user_id),
            )),
        },
        Commands::Debug { command } => match command {
            DebugCommands::Json {
                line,
                column,
                payload,
            } => Ok(Box::new(
                stacker::console::commands::debug::JsonCommand::new(line, column, payload),
            )),
            DebugCommands::Casbin {
                action,
                path,
                subject,
            } => Ok(Box::new(
                stacker::console::commands::debug::CasbinCommand::new(action, path, subject),
            )),
            DebugCommands::Dockerhub { json } => Ok(Box::new(
                stacker::console::commands::debug::DockerhubCommand::new(json),
            )),
        },
        Commands::MQ { command } => match command {
            AppMqCommands::Listen {} => Ok(Box::new(
                stacker::console::commands::mq::ListenCommand::new(),
            )),
        },
        Commands::Agent { command } => match command {
            AgentCommands::RotateToken {
                deployment_hash,
                new_token,
            } => Ok(Box::new(
                stacker::console::commands::agent::RotateTokenCommand::new(
                    deployment_hash,
                    new_token,
                ),
            )),
        },
        Commands::Stacker { command } => match command {
            StackerCommands::Login {
                org,
                domain,
                auth_url,
            } => Ok(Box::new(
                stacker::console::commands::cli::login::LoginCommand::new(org, domain, auth_url),
            )),
            StackerCommands::Init {
                app_type,
                with_proxy,
                with_ai,
                with_cloud,
                ai_provider,
                ai_model,
                ai_api_key,
            } => Ok(Box::new(
                stacker::console::commands::cli::init::InitCommand::new(
                    app_type, with_proxy, with_ai, with_cloud,
                )
                .with_ai_options(ai_provider, ai_model, ai_api_key),
            )),
            StackerCommands::Deploy {
                target,
                file,
                dry_run,
                force_rebuild,
                project,
                key,
                server,
            } => Ok(Box::new(
                stacker::console::commands::cli::deploy::DeployCommand::new(
                    target,
                    file,
                    dry_run,
                    force_rebuild,
                )
                .with_remote_overrides(project, key, server),
            )),
            StackerCommands::Logs {
                service,
                follow,
                auth_url,
                since,
                stacker::console::commands::cli::login::LoginCommand::new(org, domain, auth_url),
                stacker::console::commands::cli::logs::LogsCommand::new(
                    service, follow, tail, since,
                ),
            )),
            StackerCommands::Status { json, watch } => Ok(Box::new(
                stacker::console::commands::cli::status::StatusCommand::new(json, watch),
            )),
            StackerCommands::Destroy { volumes, confirm } => Ok(Box::new(
                stacker::console::commands::cli::destroy::DestroyCommand::new(volumes, confirm),
            )),
            StackerCommands::Config { command: cfg_cmd } => match cfg_cmd {
                StackerConfigCommands::Validate { file } => Ok(Box::new(
                    stacker::console::commands::cli::config::ConfigValidateCommand::new(file),
                )),
                StackerConfigCommands::Show { file } => Ok(Box::new(
                    stacker::console::commands::cli::config::ConfigShowCommand::new(file),
                )),
                StackerConfigCommands::Example => Ok(Box::new(
                    stacker::console::commands::cli::config::ConfigExampleCommand::new(),
                )),
                StackerConfigCommands::Fix { file, interactive } => Ok(Box::new(
                    stacker::console::commands::cli::config::ConfigFixCommand::new(file, interactive),
                )),
                StackerConfigCommands::Setup { command } => match command {
                    StackerConfigSetupCommands::Cloud { file } => Ok(Box::new(
                        stacker::console::commands::cli::config::ConfigSetupCloudCommand::new(file),
                    )),
                    StackerConfigSetupCommands::RemotePayload { file, out } => Ok(Box::new(
                        stacker::console::commands::cli::config::ConfigSetupRemotePayloadCommand::new(file, out),
                    )),
                },
            },
            StackerCommands::Ai { command: ai_cmd } => match ai_cmd {
                StackerAiCommands::Ask {
                    question,
                    context,
                    configure,
                } => Ok(Box::new(
                    stacker::console::commands::cli::ai::AiAskCommand::new(question, context)
                        .with_configure(configure),
                )),
            },
            StackerCommands::Proxy {
                command: proxy_cmd,
            } => match proxy_cmd {
                StackerProxyCommands::Add {
                    domain,
                    upstream,
                    ssl,
                } => Ok(Box::new(
                    stacker::console::commands::cli::proxy::ProxyAddCommand::new(
                        domain, upstream, ssl,
                    ),
                )),
                StackerProxyCommands::Detect => Ok(Box::new(
                    stacker::console::commands::cli::proxy::ProxyDetectCommand::new(),
                )),
            },
            StackerCommands::Update { channel } => Ok(Box::new(
                stacker::console::commands::cli::update::UpdateCommand::new(channel),
            )),
        },
    }
}
