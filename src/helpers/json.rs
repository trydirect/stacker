use serde_derive::Serialize;
#[derive(Serialize)]
pub(crate) struct JsonResponse {
    pub(crate) status: String,
    pub(crate) message: String,
    pub(crate) code: u32,
    pub(crate) id: Option<i32>,
}
