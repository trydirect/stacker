use serde_derive::{Serialize, Deserialize};
use serde_json::Value;
use serde_valid::{Validate};
use tracing_subscriber::fmt::format;
use crate::models::user::User as UserModel;

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserForm {
    pub user: User,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct User {
    #[serde(rename = "_id")]
    pub id: String,
    #[serde(rename = "first_name")]
    pub first_name: String,
    #[serde(rename = "last_name")]
    pub last_name: String,
    pub created: String,
    pub updated: String,
    pub email: String,
    #[serde(rename = "email_confirmed")]
    pub email_confirmed: bool,
    pub social: bool,
    pub website: String,
    pub currency: Value,
    pub phone: String,
    #[serde(rename = "password_change_required")]
    pub password_change_required: Value,
    pub photo: String,
    pub country: String,
    #[serde(rename = "billing_first_name")]
    pub billing_first_name: Value,
    #[serde(rename = "billing_last_name")]
    pub billing_last_name: Value,
    #[serde(rename = "billing_postcode")]
    pub billing_postcode: String,
    #[serde(rename = "billing_address_1")]
    pub billing_address_1: String,
    #[serde(rename = "billing_address_2")]
    pub billing_address_2: String,
    #[serde(rename = "billing_city")]
    pub billing_city: String,
    #[serde(rename = "billing_country_code")]
    pub billing_country_code: String,
    #[serde(rename = "billing_country_area")]
    pub billing_country_area: String,
    pub tokens: Vec<Token>,
    pub subscriptions: Vec<Subscription>,
    pub plan: Plan,
    #[serde(rename = "deployments_left")]
    pub deployments_left: Value,
    #[serde(rename = "suspension_hints")]
    pub suspension_hints: SuspensionHints,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub provider: String,
    pub expired: bool,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    #[serde(rename = "subscription_id")]
    pub subscription_id: i64,
    #[serde(rename = "user_id")]
    pub user_id: i64,
    #[serde(rename = "date_created")]
    pub date_created: String,
    #[serde(rename = "date_updated")]
    pub date_updated: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    #[serde(rename = "supported_stacks")]
    pub supported_stacks: SupportedStacks,
    #[serde(rename = "date_end")]
    pub date_end: Value,
    pub name: String,
    pub code: String,
    pub includes: Vec<Include>,
    pub team: String,
    #[serde(rename = "billing_email")]
    pub billing_email: String,
    #[serde(rename = "date_of_purchase")]
    pub date_of_purchase: String,
    pub currency: String,
    pub price: String,
    pub period: String,
    #[serde(rename = "date_start")]
    pub date_start: String,
    pub active: bool,
    #[serde(rename = "billing_id")]
    pub billing_id: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedStacks {
    pub monthly: i64,
    pub annually: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Include {
    pub name: String,
    pub code: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SuspensionHints {
    pub days: i64,
    pub reason: String,
}


impl TryInto<UserModel> for UserForm {
    type Error = String;
    fn try_into(self) -> Result<UserModel, Self::Error> {
        // let id = self.id.parse::<i32>().map_err(
        //     |msg| { format!("{:?}", msg) }
        // )?;
        Ok(UserModel {
            id: self.user.id
        })
    }

}