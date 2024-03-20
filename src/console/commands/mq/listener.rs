use crate::configuration::get_configuration;
use actix_web::rt;
use actix_web::web;
use chrono::Utc;
use lapin::{Channel, Queue};
use lapin::options::{BasicAckOptions, BasicConsumeOptions};
use lapin::types::FieldTable;
use sqlx::PgPool;
use db::deployment;
use crate::{db, forms, helpers};
use crate::helpers::{JsonResponse, mq_manager};
use crate::helpers::mq_manager::MqManager;
use futures_lite::stream::StreamExt;
use serde_derive::{Deserialize, Serialize};
use crate::forms::project::ProjectForm;

pub struct ListenCommand {
}

#[derive(Serialize, Deserialize, Debug)]
struct ProgressMessage {
    alert: i32,
    id: String,
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
            let consumer_channel= mq_manager
                .consume(
                    "install_progress",
                    "install.progress.#"
                )
                .await?;


            println!("Declare queue");
            let mut consumer = consumer_channel
                .basic_consume(
                    "#",
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

                match serde_json::from_str::<ProgressMessage>(&s) {
                    Ok(msg) => {
                        println!("message {:?}", msg);
                        // println!("id {:?}", msg.id);
                        // println!("status {:?}", msg.status);
                        delivery.ack(BasicAckOptions::default()).await.expect("ack");

                        if msg.status == "complete" {
                            let id = msg.id
                                .parse::<i32>()
                                .map_err(|err| "Could not parse deployment id".to_string() )?;

                            match crate::db::deployment::fetch(
                                db_pool.get_ref(), id
                            )
                            .await? {

                                Some(mut row) => {
                                    row.status = msg.status;
                                    row.updated_at = Utc::now();
                                    deployment::update(db_pool.get_ref(), row).await?;
                                    println!("deployment {} completed successfully", id);
                                }
                                None => println!("Deployment record not found in db")
                            }
                        }
                    }
                    Err(err) => { tracing::debug!("Invalid message format")}
                }
            }

            Ok(())
        })
    }
}
