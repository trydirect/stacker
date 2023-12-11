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
}

#[derive(Debug, Subcommand)]
enum AppClientCommands {
    New {
        #[arg(long)]
        user_id: i32,
    },
}

//todo add documentation about how to add a new command
//todo the helper from console should have a nicer display

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
        _ => Err("command does not match".to_string()),
    }
}
