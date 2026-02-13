use crate::configuration::get_configuration;
use crate::db;
use crate::helpers::mq_manager::MqManager;
use actix_web::rt;
use actix_web::web;
use chrono::Utc;
use db::deployment;
use futures_lite::stream::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions};
use lapin::types::FieldTable;
use serde_derive::{Deserialize, Serialize};
use sqlx::PgPool;
use std::time::Duration;
use tokio::time::sleep;

pub struct ListenCommand {}

use serde_json::Value;

fn string_or_number<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Value = serde::Deserialize::deserialize(deserializer)?;
    match v {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(serde::de::Error::custom("expected string or number")),
    }
}

fn optional_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v: Option<Value> = serde::Deserialize::deserialize(deserializer)?;
    match v {
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(Value::Null) | None => Ok(None),
        _ => Err(serde::de::Error::custom("expected string, number, or null")),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ProgressMessage {
    #[serde(deserialize_with = "string_or_number")]
    id: String,
    #[serde(default, deserialize_with = "optional_string_or_number")]
    deploy_id: Option<String>,
    alert: i32,
    message: String,
    status: String,
    #[serde(deserialize_with = "string_or_number")]
    progress: String,
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
            let queue_name = "stacker_listener";
            
            // Outer loop for reconnection on connection errors
            loop {
                println!("Connecting to RabbitMQ...");
                
                // Try to establish connection with retry
                let mq_manager = match Self::connect_with_retry(&settings.amqp.connection_string()).await {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to connect to RabbitMQ after retries: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };
                
                let consumer_channel = match mq_manager
                    .consume("install_progress", queue_name, "install.progress.*.*.*")
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed to create consumer: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                println!("Declare queue");
                let mut consumer = match consumer_channel
                    .basic_consume(
                        queue_name,
                        "console_listener",
                        BasicConsumeOptions::default(),
                        FieldTable::default(),
                    )
                    .await
                {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("Failed basic_consume: {}", e);
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                println!("Waiting for messages ..");
                
                // Inner loop for processing messages
                while let Some(delivery_result) = consumer.next().await {
                    let delivery = match delivery_result {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("Consumer error (will reconnect): {}", e);
                            break; // Break inner loop to reconnect
                        }
                    };
                    
                    let s: String = match String::from_utf8(delivery.data.to_owned()) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Invalid UTF-8 sequence: {}", e);
                            if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                                eprintln!("Failed to ack invalid message: {}", ack_err);
                            }
                            continue;
                        }
                    };

                    let statuses = vec![
                        "completed",
                        "paused",
                        "failed",
                        "in_progress",
                        "error",
                        "wait_resume",
                        "wait_start",
                        "confirmed",
                    ];
                    
                    match serde_json::from_str::<ProgressMessage>(&s) {
                        Ok(msg) => {
                            println!("message {:?}", s);

                            if statuses.contains(&(msg.status.as_ref())) && msg.deploy_id.is_some() {
                                println!("Update DB on status change ..");
                                let id = match msg
                                    .deploy_id
                                    .unwrap()
                                    .parse::<i32>()
                                {
                                    Ok(id) => id,
                                    Err(_) => {
                                        eprintln!("Could not parse deployment id");
                                        if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                                            eprintln!("Failed to ack: {}", ack_err);
                                        }
                                        continue;
                                    }
                                };

                                match deployment::fetch(db_pool.get_ref(), id).await {
                                    Ok(Some(mut row)) => {
                                        row.status = msg.status;
                                        row.updated_at = Utc::now();
                                        println!(
                                            "Deployment {} updated with status {}",
                                            &id, &row.status
                                        );
                                        if let Err(e) = deployment::update(db_pool.get_ref(), row).await {
                                            eprintln!("Failed to update deployment: {}", e);
                                        }
                                    }
                                    Ok(None) => println!("Deployment record was not found in db"),
                                    Err(e) => eprintln!("Failed to fetch deployment: {}", e),
                                }
                            }
                        }
                        Err(_err) => {
                            tracing::debug!("Invalid message format {:?}", _err)
                        }
                    }

                    if let Err(ack_err) = delivery.ack(BasicAckOptions::default()).await {
                        eprintln!("Failed to ack message: {}", ack_err);
                        break; // Connection likely lost, reconnect
                    }
                }
                
                println!("Consumer loop ended, reconnecting in 5s...");
                sleep(Duration::from_secs(5)).await;
            }
        })
    }
}

impl ListenCommand {
    async fn connect_with_retry(connection_string: &str) -> Result<MqManager, String> {
        let max_retries = 10;
        let mut retry_delay = Duration::from_secs(1);
        
        for attempt in 1..=max_retries {
            println!("RabbitMQ connection attempt {}/{}", attempt, max_retries);
            
            match MqManager::try_new(connection_string.to_string()) {
                Ok(manager) => {
                    println!("Connected to RabbitMQ");
                    return Ok(manager);
                }
                Err(e) => {
                    eprintln!("Connection attempt {} failed: {}", attempt, e);
                    if attempt < max_retries {
                        sleep(retry_delay).await;
                        retry_delay = std::cmp::min(retry_delay * 2, Duration::from_secs(30));
                    }
                }
            }
        }
        
        Err(format!("Failed to connect after {} attempts", max_retries))
    }
}
