use actix_web::{Responder, Result};
use serde_derive::Serialize;
use actix_web::web;
use actix_web::web::JsonBody::Error;

#[derive(Serialize)]
pub(crate) struct JsonResponse<T> {
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) code: u32,
    pub(crate) id: Option<i32>,
    pub(crate) item: Option<T>,
    pub(crate) list: Option<Vec<T>>
}


#[derive(Serialize, Default)]
pub struct JsonResponseBuilder<T>
    where T: serde::Serialize + Default
{
    status: String,
    message: String,
    code: u32,
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

    fn set_item(mut self, item:T) -> Self {
        self.item = Some(item);
        self
    }


    pub(crate) fn set_list(mut self, list:Vec<T>) -> Self {
        self.list = Some(list);
        self
    }

    pub(crate) fn ok(self) -> Result<impl Responder>  {

        Ok(web::Json(
            JsonResponse {
                status: self.status,
                message: self.message,
                code: self.code,
                id: self.id,
                item: self.item,
                list: self.list,
            }
        ))
    }

    pub(crate) fn err(self) -> Result<impl Responder>  {

        Ok(web::Json(
            JsonResponse {
                status: self.status,
                message: self.message,
                code: self.code,
                id: self.id,
                item: self.item,
                list: self.list,
            }
        ))
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
{
    pub(crate) fn new(status: String,
                      message: String,
                      code: u32,
                      id: Option<i32>,
                      item:Option<T>,
                      list: Option<Vec<T>>) -> Self {
        tracing::debug!("Executed..");
        JsonResponse {
            status,
            message,
            code,
            id,
            item,
            list,
        }
    }

    pub(crate) fn ok(id: i32, message: &str) -> JsonResponse<T> {

        let msg = if !message.trim().is_empty() {
            message.to_string()
        }
        else{
            String::from("Success")
        };

        JsonResponse {
            status: "OK".to_string(),
            message: msg,
            code: 200,
            id: Some(id),
            item: None,
            list: None,
        }
    }

    pub(crate) fn not_found() -> Self {
        JsonResponse {
            status: "Error".to_string(),
            code: 404,
            id: None,
            item: None,
            message: format!("Object not found"),
            list: None,
        }
    }

    pub(crate) fn internal_error(message: &str) -> Self {

        let msg = if !message.trim().is_empty() {
            message.to_string()
        }
        else{
            String::from("Internal error")
        };
        JsonResponse {
            status: "Error".to_string(),
            code: 500,
            id: None,
            item: None,
            message: msg,
            list: None,
        }
    }

    pub(crate) fn not_valid(message: &str) -> Self {

        let msg = if !message.trim().is_empty() {
            message.to_string()
        }
        else{
            String::from("Validation error")
        };
        JsonResponse {
            status: "Error".to_string(),
            code: 400,
            id: None,
            item: None,
            message: msg,
            list: None,
        }
    }
}