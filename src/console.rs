use actix_web::web;
use clap::{Parser, Subcommand};
use sqlx::PgPool;
use stacker::configuration::get_configuration;
use std::sync::Arc;
use tokio::runtime::Runtime;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    /*
    let db_pool = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("Failed to connect to database.");
    */
    let cli = Cli::parse();

    match cli.command {
        Commands::AppClient { command } => {
            process_app_client_command(command)?;
        }
        _ => {
            println!("other variant");
        }
    };

    Ok(())
}

fn process_app_client_command(
    command: AppClientCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let rt = Runtime::new()?;

    rt.block_on(async {
        let settings = get_configuration().expect("Failed to read configuration.");
        let db_pool = PgPool::connect(&settings.database.connection_string())
            .await
            .expect("Failed to connect to database.");

        let settings = web::Data::new(settings); //todo web::Data is already an Arc
        let db_pool = web::Data::new(db_pool);

        //todo get user from trydirect
        let user = stacker::models::user::User {
            id: "first_name".to_string(),
            first_name: "first_name".to_string(),
            last_name: "last_name".to_string(),
            email: "email".to_string(),
            email_confirmed: true,
        };
        stacker::routes::client::add_handler_inner(user, settings, db_pool)
            .await
            .expect("todo error"); //todo process the error
        Ok(())
    })
}
