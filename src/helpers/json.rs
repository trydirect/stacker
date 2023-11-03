// use std::collections::HashMap;
// use actix_web::{HttpRequest, HttpResponse, Responder};
use serde_derive::Serialize;

#[derive(Serialize)]
pub(crate) struct JsonResponse {
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) code: u32,
    pub(crate) id: Option<i32>,
}


// #[derive(Serialize)]
// pub(crate) struct JsonResponse {
//     pub(crate) status: String,
//     pub(crate) message: String,
//     pub(crate) code: u32,
//     pub(crate) custom_fields: HashMap<String, Option<String>>,
// }
//
// impl JsonResponse {
//     pub(crate) fn new(status: String, message: String, code: u32) -> Self {
//         let custom_fields = HashMap::new();
//         JsonResponse {
//             status,
//             message,
//             code,
//             custom_fields
//         }
//     }
// }
//
// // Implement the Responder trait for GlobalResponse
// impl Responder for JsonResponse {
//     type Body = ();
//
//     fn respond_to(self, _req: &HttpRequest) -> HttpResponse {
//         HttpResponse::Ok().json(self)
//     }
// }
