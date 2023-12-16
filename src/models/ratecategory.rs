use serde::{Deserialize, Serialize};

#[derive(sqlx::Type, Serialize, Deserialize, Debug, Clone, Copy)]
#[sqlx(rename_all = "lowercase", type_name = "varchar")]
pub enum RateCategory {
    Application, // app, feature, extension
    Cloud,       // is user satisfied working with this cloud
    Stack,       // app stack
    DeploymentSpeed,
    Documentation,
    Design,
    TechSupport,
    Price,
    MemoryUsage,
}

impl Into<String> for RateCategory {
    fn into(self) -> String {
        format!("{:?}", self)
    }
}

