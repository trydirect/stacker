use deadpool_lapin::{Config, CreatePoolError, Pool, Runtime};
use lapin::{
    options::*,
    publisher_confirm::{Confirmation, PublisherConfirm},
    BasicProperties, Connection, ConnectionProperties,
};

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

    pub async fn publish(
        &self,
        exchange: String,
        routing_key: String,
        payload: &[u8],
    ) -> Result<PublisherConfirm, String> {
        let connection = self.pool.get().await.map_err(|err| {
            tracing::error!("getting connection from pool {:?}", err);
            format!("getting connection from pool {:?}", err)
        })?;

        connection
            .create_channel()
            .await
            .map_err(|err| {
                tracing::error!("creating RabbitMQ channel {:?}", err);
                format!("creating RabbitMQ channel {:?}", err)
            })?
            .basic_publish(
                exchange.as_str(),
                routing_key.as_str(),
                BasicPublishOptions::default(),
                payload,
                BasicProperties::default(),
            )
            .await
            .map_err(|err| {
                tracing::error!("publishing message {:?}", err);
                format!("publishing message {:?}", err)
            })
    }

    pub async fn publish_and_confirm(
        &self,
        exchange: String,
        routing_key: String,
        payload: &[u8],
    ) -> Result<(), String> {
        self.publish(exchange, routing_key, payload)
            .await
            .map_err(|err| {
                tracing::error!("publishing the message {:?}", err);
                format!("publishing the message {:?}", err)
            })?
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
