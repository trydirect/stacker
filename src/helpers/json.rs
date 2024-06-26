use actix_web::error::{ErrorBadRequest, ErrorConflict, ErrorInternalServerError, ErrorNotFound, ErrorUnauthorized};
use actix_web::web::Json;
use actix_web::Error;
use serde_derive::Serialize;

#[derive(Serialize)]
pub(crate) struct JsonResponse<T> {
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) item: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) list: Option<Vec<T>>,
}

#[derive(Serialize, Default)]
pub struct JsonResponseBuilder<T>
where
    T: serde::Serialize + Default,
{
    message: String,
    id: Option<i32>,
    item: Option<T>,
    list: Option<Vec<T>>,
}

impl<T> JsonResponseBuilder<T>
where
    T: serde::Serialize + Default,
{
    pub(crate) fn set_msg<I: Into<String>>(mut self, msg: I) -> Self {
        self.message = msg.into();
        self
    }

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

    fn to_json_response(self) -> JsonResponse<T> {
        JsonResponse {
            message: self.message,
            id: self.id,
            item: self.item,
            list: self.list,
        }
    }

    pub(crate) fn to_string(self) -> String {
        let json_response = self.to_json_response();
        serde_json::to_string(&json_response).unwrap()
    }

    pub(crate) fn ok<I: Into<String>>(self, msg: I) -> Json<JsonResponse<T>> {
        Json(self.set_msg(msg).to_json_response())
    }

    pub(crate) fn bad_request<I: Into<String>>(
        self,
        msg: I,
    ) -> Error {
        ErrorBadRequest(self.set_msg(msg).to_string())
    }

    pub(crate) fn form_error(self, msg: String) -> Error {
        ErrorBadRequest(msg)
    }

    pub(crate) fn not_found<I: Into<String>>(self, msg: I) -> Error {
        ErrorNotFound(self.set_msg(msg).to_string())
    }

    pub(crate) fn internal_server_error<I: Into<String>>(
        self,
        msg: I,
    ) -> Error {
        ErrorInternalServerError(self.set_msg(msg).to_string())
    }

    // not used
    // pub(crate) fn unauthorized<I: Into<String>>(
    //     self,
    //     msg: I,
    // ) -> Error {
    //     ErrorUnauthorized(self.set_msg(msg).to_string())
    // }
    //
    // pub(crate) fn conflict<I: Into<String>>(self, msg: I) -> Error {
    //     ErrorConflict(self.set_msg(msg).to_string())
    // }
}

impl<T> JsonResponse<T>
where
    T: serde::Serialize + Default,
{
    pub fn build() -> JsonResponseBuilder<T> {
        JsonResponseBuilder::default()
    }
}

impl JsonResponse<String> {
    pub fn bad_request<I: Into<String>>(msg: I) -> Error {
        JsonResponse::<String>::build().bad_request( msg.into())
    }

    pub fn internal_server_error<I: Into<String>>(msg: I) -> Error {
        JsonResponse::<String>::build().internal_server_error( msg.into())
    }

    pub fn not_found<I: Into<String>>(msg: I) -> Error {
        JsonResponse::<String>::build().not_found(msg.into())
    }
}
