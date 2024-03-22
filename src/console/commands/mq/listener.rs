use crate::configuration::get_configuration;
use actix_web::rt;
use actix_web::web;
use chrono::Utc;
use lapin::options::{BasicAckOptions, BasicConsumeOptions};
use lapin::types::FieldTable;
use sqlx::PgPool;
use db::deployment;
use crate::db;
use crate::helpers::mq_manager::MqManager;
use futures_lite::stream::StreamExt;
use serde_derive::{Deserialize, Serialize};

pub struct ListenCommand {
}

#[derive(Serialize, Deserialize, Debug)]
struct ProgressMessage {
    id: String,
    deploy_id: Option<String>,
    alert: i32,
    message: String,
    status: String,
    progress: String
}

impl ListenCommand {
    pub fn new() -> Self {
        Self {}
    }
}

impl crate::console::commands::CallableTrait for ListenCommand {

    fn call(&self) -> Result<(), Box<dyn std::error::Error>> {
        rt::System::new().block_on(async {
            let settings = get_configuration().expect("Failed to read configuration.");
            let db_pool = PgPool::connect(&settings.database.connection_string())
                .await
                .expect("Failed to connect to database.");

            let db_pool = web::Data::new(db_pool);

            println!("Declare exchange");
            let mq_manager = MqManager::try_new(settings.amqp.connection_string())?;
            // let queue_name = "stacker_listener";
            let queue_name = "install_progress_m383emvfP9zQKs8lkgSU_Q";
            let consumer_channel= mq_manager
                .consume(
                    "install_progress",
                    queue_name,
                    "install.progress.*.*.*"
                )
                .await?;


            println!("Declare queue");
            let mut consumer = consumer_channel
                .basic_consume(
                    queue_name,
                    "console_listener",
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await
                .expect("Basic consume");

            println!("Waiting for messages ..");
            while let Some(delivery) = consumer.next().await {
                // println!("checking messages delivery {:?}", delivery);
                let delivery = delivery.expect("error in consumer");
                let s:String = match String::from_utf8(delivery.data.to_owned()) {
                    //delivery.data is of type Vec<u8>
                    Ok(v) => v,
                    Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
                };

                let statuses = vec![
                    "completed",
                    "paused",
                    "failed",
                    "in_progress",
                    "error",
                    "wait_resume",
                    "wait_start",
                    "confirmed"
                ];
                match serde_json::from_str::<ProgressMessage>(&s) {
                    Ok(msg) => {
                        println!("message {:?}", s);

                        if statuses.contains(&(msg.status.as_ref())) && msg.deploy_id.is_some() {
                            println!("Update DB on status change ..");
                            let id = msg.deploy_id.unwrap()
                                .parse::<i32>()
                                .map_err(|_err| "Could not parse deployment id".to_string())?;

                            match deployment::fetch(
                                db_pool.get_ref(), id
                            )
                            .await? {
                                Some(mut row) => {
                                    row.status = msg.status;
                                    row.updated_at = Utc::now();
                                    println!("Deployment {} updated with status {}",
                                         &id, &row.status
                                    );
                                    deployment::update(db_pool.get_ref(), row).await?;
                                }
                                None => println!("Deployment record was not found in db")
                            }
                        }
                    }
                    Err(_err) => { tracing::debug!("Invalid message format {:?}", _err)}
                }

                delivery.ack(BasicAckOptions::default()).await.expect("ack");
            }

            Ok(())
        })
    }
}
