use actix_web::{web, get, Responder, Result};
use sqlx::PgPool;
use crate::helpers::JsonResponse;
use crate::models;
use crate::models::user::User;


#[tracing::instrument(name = "Get logged user stack.")]
#[get("/{id}")]
pub async fn get(
    user: web::ReqData<User>,
    path: web::Path<(i32,)>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder> {
    /// Get stack apps of logged user only

    let (id,) = path.into_inner();

    tracing::info!("User {:?} gets stack by id {:?}", user.id, id);
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
            return Ok(web::Json(JsonResponse::<models::Stack>::new(
                "Success".to_string(),
                "".to_string(),
                200,
                Some(stack.id),
                Some(stack),
                None)));
        }
        Err(sqlx::Error::RowNotFound) => {
            return Ok(web::Json(JsonResponse::<models::Stack>::not_found()));
        }
        Err(e) => {
            tracing::error!("Failed to fetch stack, error: {:?}", e);
            return Ok(web::Json(JsonResponse::<models::Stack>::internal_error("")));
        }
    }
}
