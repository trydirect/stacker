use deadpool_lapin::{Config, CreatePoolError, Object, Pool, Runtime};
use lapin::{
    options::*,
    publisher_confirm::{Confirmation, PublisherConfirm},
    BasicProperties, Channel,
};
use serde::ser::Serialize;

#[derive(Debug)]
pub struct MqManager {
    pool: Pool,
}

impl MqManager {
    pub fn try_new(url: String) -> Result<Self, std::io::Error> {
        let mut cfg = Config::default();
        cfg.url = Some(url);
        let pool = cfg.create_pool(Some(Runtime::Tokio1)).map_err(|err| {
            tracing::error!("{:?}", err);

            match err {
                CreatePoolError::Config(_) => {
                    std::io::Error::new(std::io::ErrorKind::Other, "config error")
                }
                CreatePoolError::Build(_) => {
                    std::io::Error::new(std::io::ErrorKind::Other, "build error")
                }
            }
        })?;

        Ok(Self { pool })
    }

    async fn get_connection(&self) -> Result<Object, String> {
        self.pool.get().await.map_err(|err| {
            tracing::error!("getting connection from pool {:?}", err);
            format!("getting connection from pool {:?}", err)
        })
    }

    async fn create_channel(&self) -> Result<Channel, String> {
        self.get_connection()
            .await?
            .create_channel()
            .await
            .map_err(|err| {
                tracing::error!("creating RabbitMQ channel {:?}", err);
                format!("creating RabbitMQ channel {:?}", err)
            })
    }

    pub async fn publish<T: ?Sized + Serialize>(
        &self,
        exchange: String,
        routing_key: String,
        msg: &T,
    ) -> Result<PublisherConfirm, String> {
        let payload = serde_json::to_string::<T>(msg).map_err(|err| {
            format!("{:?}", err)
        })?;

        self.create_channel()
            .await?
            .basic_publish(
                exchange.as_str(),
                routing_key.as_str(),
                BasicPublishOptions::default(),
                payload.as_bytes(),
                BasicProperties::default(),
            )
            .await
            .map_err(|err| {
                tracing::error!("publishing message {:?}", err);
                format!("publishing message {:?}", err)
            })
    }

    pub async fn publish_and_confirm<T: ?Sized + Serialize>(
        &self,
        exchange: String,
        routing_key: String,
        msg: &T
    ) -> Result<(), String> {
        self.publish(exchange, routing_key, msg)
            .await?
            .await
            .map_err(|err| {
                tracing::error!("confirming the publication {:?}", err);
                format!("confirming the publication {:?}", err)
            })
            .and_then(|confirm| match confirm {
                Confirmation::NotRequested => {
                    tracing::error!("confirmation is NotRequested");
                    Err(format!("confirmation is NotRequested"))
                }
                _ => Ok(()),
            })
    }
}
