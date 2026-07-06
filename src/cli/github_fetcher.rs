use crate::cli::config_parser::ComposeHealthcheck;
use crate::cli::error::CliError;

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// GitHub URL parsing
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub fn parse_github_url(url: &str) -> Result<(String, String), CliError> {
    let trimmed = url.trim().trim_end_matches('/').trim_end_matches(".git");

    // Handle full HTTPS URL: https://github.com/owner/repo
    if trimmed.starts_with("https://github.com/") || trimmed.starts_with("http://github.com/") {
        let path = trimmed
            .split("github.com/")
            .nth(1)
            .unwrap_or("")
            .trim_start_matches('/');
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err(CliError::ConfigValidation(format!(
                "Invalid GitHub URL '{}'. Expected format: https://github.com/owner/repo or owner/repo",
                url
            )));
        }
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    // Handle shorthand: owner/repo
    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() == 2 {
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }

    Err(CliError::ConfigValidation(format!(
        "Invalid GitHub URL '{}'. Expected format: https://github.com/owner/repo or owner/repo",
        url
    )))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Infrastructure image classification
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

const INFRA_IMAGE_PREFIXES: &[&str] = &[
    "postgres",
    "mysql",
    "mariadb",
    "redis",
    "mongo",
    "rabbitmq",
    "memcached",
    "minio/",
    "bitnami/postgresql",
    "bitnami/mysql",
    "bitnami/redis",
    "clickhouse/",
    "otel/",
    "opentelemetry/",
    "grafana/",
    "prom/",
    "prometheus",
    "nginx",
    "traefik",
    "caddy",
    "jc21/nginx-proxy-manager",
    "elasticsearch",
    "kibana",
    "logstash",
];

pub fn is_infra_image(image: &str) -> bool {
    let lower = image.to_lowercase();
    let name_only = lower.split(':').next().unwrap_or(&lower);

    INFRA_IMAGE_PREFIXES
        .iter()
        .any(|prefix| name_only.starts_with(prefix))
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Default healthchecks for infra services
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

pub fn default_healthcheck(image: &str) -> Option<ComposeHealthcheck> {
    let name = image.to_lowercase();
    let name_only = name.split(':').next().unwrap_or(&name);

    if name_only.starts_with("postgres") || name_only.contains("postgresql") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL pg_isready -U postgres".to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else if name_only.starts_with("redis") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL redis-cli ping".to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else if name_only.starts_with("mysql") || name_only.contains("mysql") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL mysqladmin ping -h localhost".to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else if name_only.starts_with("mariadb") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL mysqladmin ping -h localhost".to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else if name_only.starts_with("mongo") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL mongosh --eval 'db.adminCommand(\"ping\")'".to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else if name_only.starts_with("rabbitmq") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL rabbitmq-diagnostics -q ping".to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else if name_only.starts_with("elasticsearch") {
        Some(ComposeHealthcheck {
            test: "CMD-SHELL curl -fsSL http://localhost:9200/_cluster/health || exit 1"
                .to_string(),
            interval: "5s".to_string(),
            timeout: "2s".to_string(),
            retries: 10,
        })
    } else {
        None
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
// Tests
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

#[cfg(test)]
mod tests {
    use super::*;

    // ── URL parsing tests ────────────────────────────

    #[test]
    fn test_parse_github_url_https() {
        let (owner, repo) = parse_github_url("https://github.com/octocat/hello-world").unwrap();
        assert_eq!(owner, "octocat");
        assert_eq!(repo, "hello-world");
    }

    #[test]
    fn test_parse_github_url_https_trailing_slash() {
        let (owner, repo) = parse_github_url("https://github.com/owner/repo/").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_url_dot_git_suffix() {
        let (owner, repo) =
            parse_github_url("https://github.com/ArchiveBox/ArchiveBox.git").unwrap();
        assert_eq!(owner, "ArchiveBox");
        assert_eq!(repo, "ArchiveBox");
    }

    #[test]
    fn test_parse_github_url_shorthand() {
        let (owner, repo) = parse_github_url("owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_url_shorthand_with_whitespace() {
        let (owner, repo) = parse_github_url("  owner/repo  ").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn test_parse_github_url_org_with_subgroups() {
        let (owner, repo) =
            parse_github_url("https://github.com/rust-lang-nursery/failure").unwrap();
        assert_eq!(owner, "rust-lang-nursery");
        assert_eq!(repo, "failure");
    }

    #[test]
    fn test_parse_github_url_invalid_too_short() {
        let result = parse_github_url("just-one");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Invalid GitHub URL"));
    }

    #[test]
    fn test_parse_github_url_invalid_empty() {
        let result = parse_github_url("");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Invalid GitHub URL"));
    }

    #[test]
    fn test_parse_github_url_invalid_three_parts() {
        let result = parse_github_url("a/b/c");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(err.contains("Invalid GitHub URL"));
    }

    #[test]
    fn test_parse_github_url_invalid_random_url() {
        let result = parse_github_url("https://gitlab.com/owner/repo");
        assert!(result.is_err());
    }

    // ── Infra image classification tests ────────────

    #[test]
    fn test_is_infra_image_postgres() {
        assert!(is_infra_image("postgres:16-alpine"));
        assert!(is_infra_image("postgres:16"));
        assert!(is_infra_image("postgres"));
    }

    #[test]
    fn test_is_infra_image_redis() {
        assert!(is_infra_image("redis:7-alpine"));
        assert!(is_infra_image("redis:7"));
    }

    #[test]
    fn test_is_infra_image_mysql() {
        assert!(is_infra_image("mysql:8"));
        assert!(is_infra_image("mariadb:11"));
    }

    #[test]
    fn test_is_infra_image_mongo() {
        assert!(is_infra_image("mongo:7"));
        assert!(is_infra_image("mongo:latest"));
    }

    #[test]
    fn test_is_infra_image_rabbitmq() {
        assert!(is_infra_image("rabbitmq:3-management"));
    }

    #[test]
    fn test_is_infra_image_nginx() {
        assert!(is_infra_image("nginx:alpine"));
        assert!(is_infra_image("nginx:latest"));
    }

    #[test]
    fn test_is_infra_image_grafana() {
        assert!(is_infra_image("grafana/grafana:latest"));
    }

    #[test]
    fn test_is_infra_image_bitnami() {
        assert!(is_infra_image("bitnami/postgresql:16"));
        assert!(is_infra_image("bitnami/redis:7"));
    }

    #[test]
    fn test_is_infra_image_traefik() {
        assert!(is_infra_image("traefik:latest"));
    }

    #[test]
    fn test_is_not_infra_image_app() {
        assert!(!is_infra_image("myapp:latest"));
        assert!(!is_infra_image("node:20-alpine"));
        assert!(!is_infra_image("python:3.12-slim"));
        assert!(!is_infra_image("rust:1.77-alpine"));
        assert!(!is_infra_image("golang:1.22-alpine"));
        assert!(!is_infra_image("ghcr.io/owner/myapp:latest"));
        assert!(!is_infra_image("outline/wiki:latest"));
        assert!(!is_infra_image("rustfs/rustfs:latest"));
    }

    #[test]
    fn test_is_infra_image_case_insensitive() {
        assert!(is_infra_image("Postgres:16"));
        assert!(is_infra_image("REDIS:7"));
        assert!(is_infra_image("Nginx:Alpine"));
    }

    // ── Default healthcheck tests ────────────────────

    #[test]
    fn test_default_healthcheck_postgres() {
        let hc = default_healthcheck("postgres:16-alpine").unwrap();
        assert!(hc.test.contains("pg_isready"));
        assert_eq!(hc.interval, "5s");
    }

    #[test]
    fn test_default_healthcheck_bitnami_postgres() {
        let hc = default_healthcheck("bitnami/postgresql:16").unwrap();
        assert!(hc.test.contains("pg_isready"));
    }

    #[test]
    fn test_default_healthcheck_redis() {
        let hc = default_healthcheck("redis:7-alpine").unwrap();
        assert!(hc.test.contains("redis-cli ping"));
    }

    #[test]
    fn test_default_healthcheck_mysql() {
        let hc = default_healthcheck("mysql:8").unwrap();
        assert!(hc.test.contains("mysqladmin ping"));
    }

    #[test]
    fn test_default_healthcheck_mariadb() {
        let hc = default_healthcheck("mariadb:11").unwrap();
        assert!(hc.test.contains("mysqladmin ping"));
    }

    #[test]
    fn test_default_healthcheck_mongo() {
        let hc = default_healthcheck("mongo:7").unwrap();
        assert!(hc.test.contains("mongosh"));
    }

    #[test]
    fn test_default_healthcheck_rabbitmq() {
        let hc = default_healthcheck("rabbitmq:3-management").unwrap();
        assert!(hc.test.contains("rabbitmq-diagnostics"));
    }

    #[test]
    fn test_default_healthcheck_elasticsearch() {
        let hc = default_healthcheck("elasticsearch:8.12.0").unwrap();
        assert!(hc.test.contains("_cluster/health"));
    }

    #[test]
    fn test_default_healthcheck_app_image_returns_none() {
        assert!(default_healthcheck("node:20-alpine").is_none());
        assert!(default_healthcheck("myapp:latest").is_none());
        assert!(default_healthcheck("python:3.12-slim").is_none());
    }
}
