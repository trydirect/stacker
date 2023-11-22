use actix_web::{web, get, Responder, Result};
use serde_derive::Serialize;
use sqlx::PgPool;
use crate::models;
use crate::models::user::User;

#[derive(Serialize)]
struct JsonResponse {
    status: String,
    message: String,
    code: u32,
    id: Option<i32>,
    object: Option<models::Stack>,
    objects: Option<Vec<models::Stack>>,
}

#[tracing::instrument(name = "Get stack.")]
#[get("/{id}")]
pub async fn get(
    user: web::ReqData<User>,
    path: web::Path<(i32,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {

    let (id,) = path.into_inner();

    tracing::info!("User {:?} is getting stack by id {:?}", user, id);
    match sqlx::query_as!(
        models::Stack,
        r#"
        SELECT * FROM user_stack WHERE id=$1 AND user_id=$2 LIMIT 1
        "#,
        id, user.id
    )
    .fetch_one(pool.get_ref())
    .await
    {
        Ok(stack) => {
            tracing::info!("stack found: {:?}", stack.id,);
            let response = JsonResponse {
                status: "Success".to_string(),
                code: 200,
                message: "".to_string(),
                id: Some(stack.id),
                object: Some(stack),
                objects: None
            };
            return Ok(web::Json(response));
        }
        Err(sqlx::Error::RowNotFound) => {
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 404,
                message: format!("Not Found"),
                id: None,
                object: None,
                objects: None
            }));
        }
        Err(e) => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            return Ok(web::Json(JsonResponse {
                status: "Error".to_string(),
                code: 500,
                message: format!("Internal Server Error"),
                id: None,
                object: None,
                objects: None
            }));
        }
    }
}
