pub(crate) mod hydration;
pub(crate) mod mapping;
pub(crate) mod upsert;
pub(crate) mod vault;

pub(crate) use mapping::{merge_project_app, project_app_from_post};
pub(crate) use upsert::upsert_app_config_for_deploy;
pub(crate) use vault::store_configs_to_vault_from_params;

pub(crate) fn is_compose_filename(file_name: &str) -> bool {
    matches!(
        file_name,
        "compose"
            | "compose.yml"
            | "compose.yaml"
            | "docker-compose"
            | "docker-compose.yml"
            | "docker-compose.yaml"
    )
}

#[cfg(test)]
mod tests;
