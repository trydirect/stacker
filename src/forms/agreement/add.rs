use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct AddUserAgreement {
    pub agrt_id: i32,
    pub user_id: String,
}

impl Into<models::UserAgreement> for AddUserAgreement {
    fn into(self) -> models::UserAgreement {
        let mut item = models::UserAgreement::default();
        item.agrt_id = self.agrt_id;
        item.user_id = self.user_id;
        item
    }
}
