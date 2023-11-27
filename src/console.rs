use actix_web::web;
use clap::{Parser, Subcommand};
use sqlx::PgPool;
use stacker::configuration::get_configuration;
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
    New1 {
        #[arg(long)]
        user_id: i32,
    },
}

trait StackerCommand {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>>;
}

struct AppClientNew {
    user_id: i32,
}

struct AppClientNew1 {
    user_id: i32,
}

impl AppClientNew1 {
    fn new(user_id: i32) -> Self {
        Self { user_id }
    }
}

impl StackerCommand for AppClientNew1 {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

impl AppClientNew {
    fn new(user_id: i32) -> Self {
        Self { user_id }
    }
}

impl StackerCommand for AppClientNew {
    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    /*
    let db_pool = PgPool::connect(&settings.database.connection_string())
        .await
        .expect("Failed to connect to database.");
    */
    let cli = Cli::parse();
    println!("{cli:?}");

    get_command(cli)?.call();

    Ok(())
}

fn get_command(cli: Cli) -> Result<Box<dyn StackerCommand>, String> {
    match cli.command {
        Commands::AppClient { command } => {
            //process_app_client_command(command)?;
            match command {
                AppClientCommands::New { user_id } => Ok(Box::new(AppClientNew::new(user_id))),
                AppClientCommands::New1 { user_id } => Ok(Box::new(AppClientNew1::new(user_id))),
            }
        }
        _ => Err("command does not match".to_string()),
    }
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
