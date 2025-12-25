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
    }
}
