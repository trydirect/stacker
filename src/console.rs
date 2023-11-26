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

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::AppClient { command } => {
            process_app_client_command(command);
        }
        _ => {
            println!("other variant");
        }
    };
}

fn process_app_client_command(command: AppClientCommands) {
    println!("{command:?}");
}
