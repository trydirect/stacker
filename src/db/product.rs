use sqlx::PgPool;
use crate::models;
use tracing::Instrument;

pub async fn fetch_by_obj(pg_pool: &PgPool, obj_id: i32) -> Result<models::Product, String> {
    let query_span = tracing::info_span!("Check product existence by id.");
    sqlx::query_as!(
        models::Product,
        r"SELECT * FROM product WHERE obj_id = $1",
        obj_id
    )
    .fetch_one(pg_pool)
    .instrument(query_span)
    .await
    .map_err(|e|  {
        match e {
            sqlx::Error::RowNotFound => "client not found".to_string(),
            s => {
                tracing::error!("Failed to execute fetch query: {:?}", s);
                "".to_string()
            }
        }
    })
}
