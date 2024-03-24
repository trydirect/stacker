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
    }
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
}

#[derive(Debug, Subcommand)]
enum AppMqCommands {
    Listen {
    },
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
            DebugCommands::Json { line, column, payload } => Ok(Box::new(
                stacker::console::commands::debug::JsonCommand::new(line, column, payload),
            )),
        },
        Commands::MQ { command} => match command {
            AppMqCommands::Listen {} => Ok(Box::new(
                stacker::console::commands::mq::ListenCommand::new(),
            )),
        }
    }
}
