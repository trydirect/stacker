use crate::models;
use serde::{Deserialize, Serialize};
use serde_valid::Validate;

#[derive(Serialize, Deserialize, Debug, Validate)]
pub struct Rating {
    pub obj_id: i32,                    // product external id
    pub category: models::RateCategory, // rating of product | rating of service etc
    #[validate(max_length = 1000)]
    pub comment: Option<String>, // always linked to a product
    #[validate(minimum = 0)]
    #[validate(maximum = 10)]
    pub rate: i32, //
}

impl Into<models::Rating> for Rating {
    fn into(self) -> models::Rating {
        let mut rating = models::Rating::default(); 
        rating.obj_id = self.obj_id;
        rating.category = self.category.into(); //todo change the type of category field to the RateCategory
        rating.hidden = Some(false); 
        rating.rate = Some(self.rate);
        rating.comment = self.comment; 

        rating
    }
}
