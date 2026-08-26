const DEFAULT_PROJECT: &str = "project";

pub const REMOTE_RUNTIME_ENV_FILE: &str = ".env";

pub fn remote_runtime_env_path_for(stack_code: &str) -> String {
    format!("/home/trydirect/{}/.env", stack_code)
}

pub fn remote_runtime_compose_path_for(stack_code: &str) -> String {
    format!("/home/trydirect/{}/docker-compose.yml", stack_code)
}

pub fn compose_env_file_reference() -> &'static str {
    REMOTE_RUNTIME_ENV_FILE
}

pub fn remote_runtime_env_path() -> String {
    remote_runtime_env_path_for(DEFAULT_PROJECT)
}

pub fn remote_runtime_compose_path() -> String {
    remote_runtime_compose_path_for(DEFAULT_PROJECT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_runtime_env_path_is_canonical() {
        assert_eq!(remote_runtime_env_path(), "/home/trydirect/project/.env");
    }

    #[test]
    fn compose_env_file_reference_is_relative() {
        assert_eq!(compose_env_file_reference(), ".env");
    }

    #[test]
    fn remote_runtime_compose_path_is_canonical() {
        assert_eq!(
            remote_runtime_compose_path(),
            "/home/trydirect/project/docker-compose.yml"
        );
    }

    #[test]
    fn remote_runtime_env_path_for_custom_stack() {
        assert_eq!(
            remote_runtime_env_path_for("my-app"),
            "/home/trydirect/my-app/.env"
        );
    }

    #[test]
    fn remote_runtime_compose_path_for_custom_stack() {
        assert_eq!(
            remote_runtime_compose_path_for("my-app"),
            "/home/trydirect/my-app/docker-compose.yml"
        );
    }
}
