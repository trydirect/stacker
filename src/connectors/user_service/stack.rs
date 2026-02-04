use serde::Deserialize;

use crate::connectors::errors::ConnectorError;

use super::app::Application;
use super::UserServiceClient;

#[derive(Debug, Deserialize)]
pub(crate) struct StackViewItem {
    pub(crate) code: String,
    pub(crate) value: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StackViewResponse {
    pub(crate) _items: Vec<StackViewItem>,
}

impl UserServiceClient {
    pub(crate) async fn search_stack_view(
        &self,
        bearer_token: &str,
        query: Option<&str>,
    ) -> Result<Vec<Application>, ConnectorError> {
        let url = format!("{}/stack_view", self.base_url);
        let response = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", bearer_token))
            .send()
            .await
            .map_err(ConnectorError::from)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(ConnectorError::HttpError(format!(
                "User Service error ({}): {}",
                status, body
            )));
        }

        let wrapper: StackViewResponse = response
            .json()
            .await
            .map_err(|e| ConnectorError::InvalidResponse(e.to_string()))?;

        let mut apps: Vec<Application> = wrapper
            ._items
            .into_iter()
            .map(application_from_stack_view)
            .collect();

        if let Some(q) = query {
            let q = q.to_lowercase();
            apps.retain(|app| {
                let name = app.name.as_deref().unwrap_or("").to_lowercase();
                let code = app.code.as_deref().unwrap_or("").to_lowercase();
                name.contains(&q) || code.contains(&q)
            });
        }

        Ok(apps)
    }
}

pub(crate) fn application_from_stack_view(item: StackViewItem) -> Application {
    let value = item.value;
    let id = value.get("_id").and_then(|v| v.as_i64());
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let code = value
        .get("code")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| Some(item.code));
    let description = value
        .get("description")
        .or_else(|| value.get("_description"))
        .or_else(|| value.get("full_description"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let category = value
        .get("module")
        .or_else(|| value.get("category"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let docker_image = value
        .get("image")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            value
                .get("images")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let default_port = value
        .get("ports")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|port| {
            port.get("container")
                .or_else(|| port.get("host"))
                .and_then(|v| v.as_i64())
        })
        .map(|v| v as i32);

    Application {
        id,
        name,
        code,
        description,
        category,
        docker_image,
        default_port,
    }
}
