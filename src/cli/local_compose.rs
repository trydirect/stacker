use std::path::{Path, PathBuf};

use crate::cli::config_parser::StackerConfig;
use crate::cli::error::CliError;

const OUTPUT_DIR: &str = ".stacker";
const DEFAULT_CONFIG_FILE: &str = "stacker.yml";

pub fn resolve_local_compose_path(project_dir: &Path) -> Result<PathBuf, CliError> {
    let generated = project_dir.join(OUTPUT_DIR).join("docker-compose.yml");
    let config_path = project_dir.join(DEFAULT_CONFIG_FILE);

    if config_path.exists() {
        if let Ok(config) = StackerConfig::from_file(&config_path) {
            if let Some(compose_file) = config.deploy.compose_file {
                let resolved = if compose_file.is_absolute() {
                    compose_file
                } else {
                    project_dir.join(compose_file)
                };
                if resolved.exists() {
                    return Ok(resolved);
                }
            }
        }
    }

    if generated.exists() {
        return Ok(generated);
    }

    Err(CliError::ConfigValidation(
        "No deployment found. Run 'stacker deploy' first.".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_local_compose_path_prefers_configured_compose_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("docker/local")).unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join("docker/local/compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("stacker.yml"),
            "name: demo\ndeploy:\n  target: local\n  compose_file: docker/local/compose.yml\n",
        )
        .unwrap();

        let resolved = resolve_local_compose_path(dir.path()).unwrap();
        assert_eq!(resolved, dir.path().join("docker/local/compose.yml"));
    }

    #[test]
    fn test_resolve_local_compose_path_falls_back_to_generated_compose() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".stacker")).unwrap();
        std::fs::write(
            dir.path().join(".stacker/docker-compose.yml"),
            "services: {}\n",
        )
        .unwrap();

        let resolved = resolve_local_compose_path(dir.path()).unwrap();
        assert_eq!(resolved, dir.path().join(".stacker/docker-compose.yml"));
    }
}
