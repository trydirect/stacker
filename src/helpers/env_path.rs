pub const REMOTE_RUNTIME_ENV_PATH: &str = "/home/trydirect/project/.env";
pub const REMOTE_RUNTIME_ENV_FILE: &str = ".env";

pub fn remote_runtime_env_path() -> &'static str {
    REMOTE_RUNTIME_ENV_PATH
}

pub fn compose_env_file_reference() -> &'static str {
    REMOTE_RUNTIME_ENV_FILE
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
}
