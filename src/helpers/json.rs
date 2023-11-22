use actix_web::{Responder, Result};
use actix_web::error::{ErrorBadRequest, ErrorConflict, ErrorNotFound, ErrorInternalServerError};
use serde_derive::Serialize;
use actix_web::web::Json;
use actix_web::Error;

#[derive(Serialize)]
pub(crate) struct JsonResponse<T> {
    pub(crate) message: String,
    pub(crate) id: Option<i32>,
    pub(crate) item: Option<T>,
    pub(crate) list: Option<Vec<T>>
}


#[derive(Serialize, Default)]
pub struct JsonResponseBuilder<T>
    where T: serde::Serialize + Default
{
    id: Option<i32>,
    item: Option<T>,
    list: Option<Vec<T>>
}

impl<T> JsonResponseBuilder<T>
    where T: serde::Serialize + Default
{
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn set_item(mut self, item:T) -> Self {
        self.item = Some(item);
        self
    }

    pub(crate) fn set_id(mut self, id:i32) -> Self {
        self.id = Some(id);
        self
    }

    pub(crate) fn set_list(mut self, list:Vec<T>) -> Self {
        self.list = Some(list);
        self
    }

    pub(crate) fn ok(self, msg: String) -> Result<Json<JsonResponse<T>>, Error>  {

        Ok(Json(
            JsonResponse {
                message: msg,
                id: self.id,
                item: self.item,
                list: self.list,
            }
        ))

    }

    pub(crate) fn err(self, msg: String) -> Result<Json<JsonResponse<T>>, Error>  {

        let json_response = JsonResponse {
            message: msg,
            id: self.id,
            item: self.item,
            list: self.list
        };

        Err(ErrorBadRequest(
            serde_json::to_string(&json_response).unwrap()))
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
}

impl<T> From<T> for JsonResponseBuilder<T>
    where T: serde::Serialize + Default {
    fn from(value: T) -> Self {
        JsonResponseBuilder::default().set_item(value)
    }
}

impl<T> From<Vec<T>> for JsonResponseBuilder<T>
    where T: serde::Serialize + Default {
    fn from(value: Vec<T>) -> Self {
        JsonResponseBuilder::default().set_list(value)
    }
}

impl<T> JsonResponse<T>
where T: serde::Serialize + Default
{
    pub fn build() -> JsonResponseBuilder<T>
    {
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

    // pub(crate) fn ok(id: i32, message: &str) -> JsonResponse<T> {
    //
    //     let msg = if !message.trim().is_empty() {
    //         message.to_string()
    //     }
    //     else{
    //         String::from("Success")
    //     };
    //
    //     JsonResponse {
    //         message: msg,
    //         id: Some(id),
    //         item: None,
    //         list: None,
    //     }
    // }

    // pub(crate) fn not_found() -> Self {
    //     JsonResponse {
    //         id: None,
    //         item: None,
    //         message: format!("Object not found"),
    //         list: None,
    //     }
    // }
    //
    // pub(crate) fn internal_error(message: &str) -> Self {
    //
    //     let msg = if !message.trim().is_empty() {
    //         message.to_string()
    //     }
    //     else{
    //         String::from("Internal error")
    //     };
    //     JsonResponse {
    //         id: None,
    //         item: None,
    //         message: msg,
    //         list: None,
    //     }
    // }
    //
    // pub(crate) fn not_valid(message: &str) -> Self {
    //
    //     let msg = if !message.trim().is_empty() {
    //         message.to_string()
    //     }
    //     else{
    //         String::from("Validation error")
    //     };
    //     JsonResponse {
    //         id: None,
    //         item: None,
    //         message: msg,
    //         list: None,
    //     }
    // }
}