use crate::configuration::get_configuration;
use actix_web::rt;
use actix_web::web;
use lapin::{Channel, Queue};
use lapin::options::{BasicAckOptions, BasicConsumeOptions};
use lapin::types::FieldTable;
use sqlx::PgPool;
use db::deployment;
use crate::{db, helpers};
use crate::helpers::mq_manager;
use crate::helpers::mq_manager::MqManager;

pub struct ListenCommand {
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

            let mq_manager = MqManager::try_new(settings.amqp.connection_string())?;
            let consumer_channel= mq_manager
                .consume(
                    "install_progress",
                    "install_progress_*******"
                )
                .await?;


            let consumer = consumer_channel
                .basic_consume(
                    "install_progress",
                    "console_listener",
                    BasicConsumeOptions::default(),
                    FieldTable::default(),
                )
                .await
                .expect("Basic consume");

                // .map_err(|err| format!("Error {:?}", err));

            tracing::info!("will consume");
            // if let Ok(consumer) = consumer {
            //     while let Some(delivery) = consumer.next().await {
            //         let delivery = delivery.expect("error in consumer");
            //         delivery.ack(BasicAckOptions::default()).await.expect("ack");
            //     }
            // }

            // while let Some(delivery) = consumer.next().await {
            //     tracing::debug!(message=?delivery, "received message");
            //     if let Ok(delivery) = delivery {
            //         delivery
            //             .ack(BasicAckOptions::default())
            //             .await
            //             .expect("basic_ack");
            //     }
            // }


            // on_complete()
            // let deployment = crate::models::deployment::Deployment {
            //     id: 0,
            //     project_id: 0,
            //     deleted: false,
            //     status: "".to_string(),
            //     body: Default::default(),
            //     created_at: Default::default(),
            //     updated_at: Default::default(),
            // };
            // deployment::update(db_pool.get_ref(), deployment).await?;

            Ok(())
        })
    }
}
