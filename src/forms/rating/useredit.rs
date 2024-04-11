use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct UserEditRating {
    #[validate(max_length = 1000)]
    pub comment: Option<String>, // always linked to a product
    #[validate(minimum = 0)]
    #[validate(maximum = 10)]
    pub rate: i32, //
}
