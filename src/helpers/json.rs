use actix_web::error::{ErrorBadRequest, ErrorConflict, ErrorNotFound, ErrorInternalServerError};
use serde_derive::Serialize;
use actix_web::web::Json;
use actix_web::Error;
use actix_web::Result;

#[derive(Serialize)]
pub(crate) struct JsonResponse<T> {
    pub(crate) message: String,
    pub(crate) id: Option<i32>,
    pub(crate) item: Option<T>,
    pub(crate) list: Option<Vec<T>>
}

#[derive(Serialize, Default)]
pub struct JsonResponseBuilder<T>
where
    T: serde::Serialize + Default,
{
    id: Option<i32>,
    item: Option<T>,
    list: Option<Vec<T>>,
}

impl<T> JsonResponseBuilder<T>
where
    T: serde::Serialize + Default,
{
    pub(crate) fn set_item(mut self, item: T) -> Self {
        self.item = Some(item);
        self
    }

    pub(crate) fn set_id(mut self, id: i32) -> Self {
        self.id = Some(id);
        self
    }

    pub(crate) fn set_list(mut self, list: Vec<T>) -> Self {
        self.list = Some(list);
        self
    }

    fn to_json_response(self, msg: String) -> JsonResponse<T> {
        JsonResponse {
            message: msg,
            id: self.id,
            item: self.item,
            list: self.list,
        }
    }

    pub(crate) fn ok<I: Into<String>>(self, msg: I) -> Result<Json<JsonResponse<T>>, Error> {
        Ok(Json(self.to_json_response(msg.into())))
    }

    pub(crate) fn err<I: Into<String>>(self, msg: I) -> Result<Json<JsonResponse<T>>, Error> {
        let json_response = self.to_json_response(msg.into());

        Err(ErrorBadRequest(
            serde_json::to_string(&json_response).unwrap(),
        ))
    }

    pub(crate) fn not_found(self, msg: String) -> Result<Json<JsonResponse<T>>, Error>  {

        let json_response = JsonResponse {
            message: msg,
            id: self.id,
            item: self.item,
            list: self.list
        };

        Err(ErrorNotFound(
            serde_json::to_string(&json_response).unwrap()))
    }

    pub(crate) fn internal_error(self, msg: String) -> Result<Json<JsonResponse<T>>, Error>  {

        let json_response = JsonResponse {
            message: msg,
            id: self.id,
            item: self.item,
            list: self.list
        };

        Err(ErrorInternalServerError(
            serde_json::to_string(&json_response).unwrap()))
    }

    pub(crate) fn conflict(self, msg: String) -> Result<Json<JsonResponse<T>>, Error>  {

        let json_response = JsonResponse {
            message: msg,
            id: self.id,
            item: self.item,
            list: self.list
        };

        Err(ErrorConflict(
            serde_json::to_string(&json_response).unwrap()))
    }

    pub(crate) fn err_internal_server_error<I: Into<String>>(
        self,
        msg: I,
    ) -> Result<Json<JsonResponse<T>>, Error> {
        let json_response = self.to_json_response(msg.into());

        Err(ErrorInternalServerError(
            serde_json::to_string(&json_response).unwrap(),
        ))
    }
}

impl<T> JsonResponse<T>
where
    T: serde::Serialize + Default,
{
    pub fn build() -> JsonResponseBuilder<T> {
        JsonResponseBuilder::default()
    }

    pub(crate) fn new(message: String,
                      id: Option<i32>,
                      item:Option<T>,
                      list: Option<Vec<T>>) -> Self {
        tracing::debug!("Executed..");
        JsonResponse {
            message,
            id,
            item,
            list,
        }
    }
}
