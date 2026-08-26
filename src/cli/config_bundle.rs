use std::collections::BTreeMap;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};
use zstd::stream::write::Encoder;

use crate::cli::error::CliError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigBundleFile {
    pub source_path: String,
    pub destination_path: String,
    pub mode: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfigBundleManifest {
    pub version: u32,
    pub environment: String,
    pub files: Vec<ConfigBundleFile>,
}

#[derive(Debug, Clone)]
pub struct ConfigBundleArtifacts {
    pub environment: String,
    pub manifest_path: PathBuf,
    pub archive_path: PathBuf,
    pub remote_compose_path: PathBuf,
    pub manifest: ConfigBundleManifest,
    pub config_files: Vec<serde_json::Value>,
    /// True when `environment` is a synthetic namespace fallback (no environment
    /// profile was selected). Used only as an internal bundle namespace and must
    /// not be surfaced as the deploy-time environment.
    pub synthesized_environment: bool,
}

impl ConfigBundleArtifacts {
    pub fn artifact_metadata(&self) -> serde_json::Value {
        let files: Vec<serde_json::Value> = self
            .manifest
            .files
            .iter()
            .map(|file| {
                json!({
                    "source_path": file.source_path,
                    "destination_path": file.destination_path,
                    "mode": file.mode,
                    "size": file.size,
                    "sha256": file.sha256,
                    "content_hidden": is_secret_like_path(&file.source_path),
                })
            })
            .collect();

        json!({
            "environment": self.environment,
            "manifest_path": self.manifest_path.to_string_lossy(),
            "archive_path": self.archive_path.to_string_lossy(),
            "remote_compose_path": self.remote_compose_path.to_string_lossy(),
            "config_files": files,
        })
    }
}

pub fn build_config_bundle(
    project_dir: &Path,
    environment: &str,
    compose_path: &Path,
    env_file: Option<&Path>,
    reference_base: &Path,
    attach_agent_network: bool,
) -> Result<ConfigBundleArtifacts, CliError> {
    validate_environment_name(environment)?;

    let project_root = project_dir.canonicalize()?;
    let compose_canonical = compose_path.canonicalize()?;
    ensure_inside_project(&project_root, &compose_canonical)?;
    // Relative bind-mount / env_file references inside the compose resolve against
    // `reference_base`. For a user-supplied compose this is the compose's own
    // directory (standard Docker Compose semantics); for a generated compose living
    // under `.stacker/`, the caller passes the project root, because the paths were
    // authored in stacker.yml relative to the project root, not the output dir.
    let reference_base = reference_base.canonicalize().map_err(|err| {
        validation_error(format!(
            "config bundle reference base directory does not exist or cannot be read: {} ({})",
            reference_base.display(),
            err
        ))
    })?;
    ensure_inside_project(&project_root, &reference_base)?;

    let output_dir = project_root.join(".stacker/deploy").join(environment);
    std::fs::create_dir_all(&output_dir)?;
    let manifest_path = output_dir.join("config-bundle.manifest.json");
    let archive_path = output_dir.join("config-bundle.tar.zst");
    let remote_compose_path = output_dir.join("docker-compose.remote.yml");

    let compose_content = std::fs::read_to_string(&compose_canonical)?;
    let mut compose_yaml: serde_yaml::Value = serde_yaml::from_str(&compose_content)?;

    // Drop platform-managed services (e.g. the nginx-proxy-manager ingress)
    // from the compose that ships to the remote host. Platform-managed
    // services are deployed by their own install-service Ansible role into
    // their own directory (`/home/trydirect/<service>/`), NOT inside the
    // project compose. Leaving them here too would deploy the same container
    // twice and collide on the ingress host ports (80/443/81) — the
    // "duplicate runtime ownership" that the scope convention in
    // docs/APP_DEPLOYMENT.md exists to prevent. This runs only when building
    // the remote bundle, so the local `.stacker/docker-compose.yml` keeps the
    // proxy service (a local deploy has no install-service role to run it).
    let stripped_platform_services = strip_platform_managed_services(&mut compose_yaml);
    if !stripped_platform_services.is_empty() {
        eprintln!(
            "  Excluding platform-managed service(s) from the remote compose \
             (installed separately by their own role): {}",
            stripped_platform_services.join(", ")
        );
    }

    let mut collected = BTreeMap::<PathBuf, CollectedFile>::new();

    let selected_env_file = if let Some(env_file) = env_file {
        let resolved = resolve_reference_path(&project_root, &project_root, env_file)?;
        collect_file(&project_root, environment, resolved.clone(), &mut collected)?;
        Some(resolved)
    } else {
        None
    };

    rewrite_compose_references(
        &project_root,
        &reference_base,
        environment,
        &mut compose_yaml,
        &mut collected,
    )?;

    // Status-panel/agent deploys: put every project service on the shared
    // external `default_network` the agent runs on, so it can reach containers
    // by name/IP. Idempotent — services already on the network are untouched.
    if attach_agent_network {
        crate::cli::compose_service_sync::inject_shared_network_all_services(
            &mut compose_yaml,
            "default_network",
        );
    }

    let rewritten_compose = serde_yaml::to_string(&compose_yaml)
        .map_err(|err| validation_error(format!("failed to write remote compose: {err}")))?;
    std::fs::write(&remote_compose_path, &rewritten_compose)?;

    let mut files: Vec<ConfigBundleFile> = collected
        .values()
        .map(|file| ConfigBundleFile {
            source_path: file.source_path.clone(),
            destination_path: file.destination_path.clone(),
            mode: file.mode.clone(),
            size: file.bytes.len() as u64,
            sha256: sha256_hex(&file.bytes),
        })
        .collect();
    files.sort_by(|left, right| left.source_path.cmp(&right.source_path));
    validate_relative_destinations(&files)?;

    let manifest = ConfigBundleManifest {
        version: 1,
        environment: environment.to_string(),
        files,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| validation_error(format!("failed to serialize manifest: {err}")))?;
    std::fs::write(&manifest_path, manifest_json)?;
    write_archive(&archive_path, collected.values())?;

    let mut config_files = Vec::new();
    config_files.push(json!({
        "name": "docker-compose.yml",
        "content": rewritten_compose,
        "content_type": "application/x-yaml",
        "destination_path": "docker-compose.yml",
        "file_mode": "0644",
        "owner": "root",
        "group": "root"
    }));

    if let Some(selected_env_file) = selected_env_file.as_ref() {
        let canonical = selected_env_file.canonicalize()?;
        let collected_env_file = collected
            .get(&canonical)
            .expect("selected env file should be present in collected bundle");
        let compose_env_content =
            String::from_utf8(collected_env_file.bytes.clone()).map_err(|_| {
                validation_error(format!(
                    "config file '{}' must be UTF-8 text to upload in the deploy payload",
                    collected_env_file.source_path
                ))
            })?;
        config_files.push(json!({
            "name": ".env",
            "content": compose_env_content,
            "content_type": "text/plain",
            "destination_path": ".env",
            "file_mode": collected_env_file.mode,
            "owner": "root",
            "group": "root"
        }));
    }

    for file in collected.values() {
        let content = String::from_utf8(file.bytes.clone()).map_err(|_| {
            validation_error(format!(
                "config file '{}' must be UTF-8 text to upload in the deploy payload",
                file.source_path
            ))
        })?;
        config_files.push(json!({
            "name": Path::new(&file.source_path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(file.source_path.as_str()),
            "content": content,
            "content_type": "text/plain",
            "destination_path": file.destination_path,
            "file_mode": file.mode,
            "owner": "root",
            "group": "root"
        }));
    }

    Ok(ConfigBundleArtifacts {
        environment: environment.to_string(),
        manifest_path,
        archive_path,
        remote_compose_path,
        manifest,
        config_files,
        synthesized_environment: false,
    })
}

#[derive(Debug, Clone)]
struct CollectedFile {
    source_path: String,
    destination_path: String,
    mode: String,
    bytes: Vec<u8>,
}

fn rewrite_compose_references(
    project_root: &Path,
    compose_dir: &Path,
    environment: &str,
    compose_yaml: &mut serde_yaml::Value,
    collected: &mut BTreeMap<PathBuf, CollectedFile>,
) -> Result<(), CliError> {
    let Some(services) = mapping_mut(compose_yaml)
        .and_then(|root| root.get_mut(serde_yaml::Value::String("services".to_string())))
        .and_then(mapping_mut)
    else {
        return Ok(());
    };

    for service in services.values_mut() {
        let Some(service_map) = mapping_mut(service) else {
            continue;
        };

        if let Some(env_file_value) =
            service_map.get_mut(serde_yaml::Value::String("env_file".to_string()))
        {
            rewrite_env_file(
                project_root,
                compose_dir,
                environment,
                env_file_value,
                collected,
            )?;
        }

        if let Some(volumes_value) =
            service_map.get_mut(serde_yaml::Value::String("volumes".to_string()))
        {
            rewrite_volumes(
                project_root,
                compose_dir,
                environment,
                volumes_value,
                collected,
            )?;
        }
    }

    Ok(())
}

fn rewrite_env_file(
    project_root: &Path,
    compose_dir: &Path,
    environment: &str,
    value: &mut serde_yaml::Value,
    collected: &mut BTreeMap<PathBuf, CollectedFile>,
) -> Result<(), CliError> {
    match value {
        serde_yaml::Value::String(path) => {
            let remote =
                collect_reference(project_root, compose_dir, environment, path, collected)?;
            *path = remote;
        }
        serde_yaml::Value::Sequence(values) => {
            for item in values {
                if let serde_yaml::Value::String(path) = item {
                    let remote =
                        collect_reference(project_root, compose_dir, environment, path, collected)?;
                    *path = remote;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn rewrite_volumes(
    project_root: &Path,
    compose_dir: &Path,
    environment: &str,
    value: &mut serde_yaml::Value,
    collected: &mut BTreeMap<PathBuf, CollectedFile>,
) -> Result<(), CliError> {
    let serde_yaml::Value::Sequence(volumes) = value else {
        return Ok(());
    };

    for volume in volumes {
        if let serde_yaml::Value::String(volume_spec) = volume {
            // Simple string form: "source:target:mode"
            let Some((source, rest)) = parse_bind_mount(volume_spec) else {
                continue;
            };
            // Only project-local files are bundled. Directory mounts, host
            // paths, and not-present sources are runtime volumes managed on the
            // target host, so leave the entry unchanged.
            if let Some(remote) =
                collect_volume_reference(project_root, compose_dir, environment, source, collected)?
            {
                *volume_spec = format!("{remote}:{rest}");
            }
        } else if let serde_yaml::Value::Mapping(map) = volume {
            // Advanced mapping form: { type: bind, source: ..., target: ... }
            let vol_type = map
                .get(&serde_yaml::Value::String("type".to_string()))
                .and_then(|v| v.as_str());
            if vol_type != Some("bind") {
                continue;
            }
            let source_key = serde_yaml::Value::String("source".to_string());
            let source_val = match map.get(&source_key).and_then(|v| v.as_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };
            if let Some(remote) = collect_volume_reference(
                project_root,
                compose_dir,
                environment,
                &source_val,
                collected,
            )? {
                map.insert(source_key, serde_yaml::Value::String(remote));
            }
        }
    }

    Ok(())
}

fn parse_bind_mount(volume_spec: &str) -> Option<(&str, &str)> {
    let (source, rest) = volume_spec.split_once(':')?;
    if source.starts_with('.')
        || source.starts_with('/')
        || source.starts_with('~')
        || source.contains(std::path::MAIN_SEPARATOR)
    {
        Some((source, rest))
    } else {
        None
    }
}

fn collect_reference(
    project_root: &Path,
    base_dir: &Path,
    environment: &str,
    reference: &str,
    collected: &mut BTreeMap<PathBuf, CollectedFile>,
) -> Result<String, CliError> {
    let resolved = resolve_reference_path(project_root, base_dir, Path::new(reference))?;
    let collected_file = collect_file(project_root, environment, resolved, collected)?;
    let mut dest = collected_file.destination_path.clone();
    // Docker Compose treats a bare name (no ./ or / prefix) as a named volume,
    // not a bind mount. If the original reference was relative, re-prefix so
    // the rewritten compose file still produces a bind mount.
    if reference.starts_with("./") && !dest.starts_with("./") {
        dest.insert_str(0, "./");
    }
    Ok(dest)
}

/// Resolve a bind-mount source for a config bundle. A source is bundled only
/// when it is a **project-local file**; that file is collected and the rewritten
/// remote path is returned as `Some`.
///
/// Everything else is a runtime volume the target host manages, so it is left
/// untouched (returns `None`) instead of failing the deploy:
///   - directory mounts (e.g. `./library`, `./config`) — created on the target,
///   - absolute / host paths and `~`-relative paths,
///   - sources that resolve outside the project directory,
///   - sources that do not exist locally (created on the target).
///
/// This is intentionally more lenient than [`collect_reference`], which stays
/// strict for `env_file:` entries (those must exist and be files).
fn collect_volume_reference(
    project_root: &Path,
    base_dir: &Path,
    environment: &str,
    reference: &str,
    collected: &mut BTreeMap<PathBuf, CollectedFile>,
) -> Result<Option<String>, CliError> {
    let path = Path::new(reference);
    if path.is_absolute() || reference.starts_with('~') {
        return Ok(None);
    }

    // Resolve without erroring on missing paths (missing = created on target).
    let Ok(canonical) = base_dir.join(path).canonicalize() else {
        return Ok(None);
    };

    if !canonical.starts_with(project_root) {
        return Ok(None);
    }

    if canonical.is_dir() {
        eprintln!(
            "  Note: skipping directory bind mount '{}' — it is created on the target host, not uploaded.",
            reference
        );
        return Ok(None);
    }

    if !canonical.is_file() {
        return Ok(None);
    }

    Ok(Some(collect_reference(
        project_root,
        base_dir,
        environment,
        reference,
        collected,
    )?))
}

fn collect_file<'a>(
    project_root: &Path,
    _environment: &str,
    path: PathBuf,
    collected: &'a mut BTreeMap<PathBuf, CollectedFile>,
) -> Result<&'a CollectedFile, CliError> {
    let canonical = path.canonicalize().map_err(|err| {
        validation_error(format!(
            "config bundle referenced file does not exist or cannot be read: {} ({})",
            path.display(),
            err
        ))
    })?;
    ensure_inside_project(project_root, &canonical)?;

    if canonical.is_dir() {
        return Err(validation_error(format!(
            "directory mounts are not supported in config bundles: {}",
            display_project_path(project_root, &canonical)
        )));
    }

    if !canonical.is_file() {
        return Err(validation_error(format!(
            "config bundle path is not a file: {}",
            canonical.display()
        )));
    }

    if !collected.contains_key(&canonical) {
        let source_path = display_project_path(project_root, &canonical);
        let destination_path = source_path.replace('\\', "/");
        collected.insert(
            canonical.clone(),
            CollectedFile {
                source_path,
                destination_path,
                mode: "0644".to_string(),
                bytes: std::fs::read(&canonical).map_err(|err| {
                    validation_error(format!(
                        "failed to read config bundle file {}: {}",
                        display_project_path(project_root, &canonical),
                        err
                    ))
                })?,
            },
        );
    }

    Ok(collected
        .get(&canonical)
        .expect("collected file was inserted"))
}

fn validate_relative_destinations(files: &[ConfigBundleFile]) -> Result<(), CliError> {
    for file in files {
        if Path::new(&file.destination_path).is_absolute() {
            return Err(validation_error(format!(
                "config bundle destination must be project-relative: {} -> {}",
                file.source_path, file.destination_path
            )));
        }
    }

    Ok(())
}

fn write_archive<'a>(
    archive_path: &Path,
    files: impl IntoIterator<Item = &'a CollectedFile>,
) -> Result<(), CliError> {
    let archive_file = File::create(archive_path)?;
    let encoder = Encoder::new(archive_file, 0)
        .map_err(|err| validation_error(format!("failed to create zstd archive: {err}")))?;
    let mut tar = Builder::new(encoder);

    for file in files {
        let mut header = Header::new_gnu();
        header.set_size(file.bytes.len() as u64);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_cksum();
        tar.append_data(&mut header, &file.source_path, file.bytes.as_slice())?;
    }

    let encoder = tar.into_inner()?;
    encoder
        .finish()
        .map_err(|err| validation_error(format!("failed to finish zstd archive: {err}")))?;
    Ok(())
}

fn resolve_reference_path(
    project_root: &Path,
    base_dir: &Path,
    reference: &Path,
) -> Result<PathBuf, CliError> {
    if reference.is_absolute() {
        return Ok(reference.to_path_buf());
    }

    if reference.starts_with("~") {
        return Err(validation_error(format!(
            "home-relative config paths are not supported: {}",
            reference.display()
        )));
    }

    let base = if reference
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        let joined = base_dir.join(reference);
        joined.canonicalize().map_err(|err| {
            validation_error(format!(
                "config bundle referenced file does not exist or cannot be read: {} ({})",
                joined.display(),
                err
            ))
        })?
    } else {
        base_dir.join(reference)
    };

    let canonical = base.canonicalize().map_err(|err| {
        validation_error(format!(
            "config bundle referenced file does not exist or cannot be read: {} ({})",
            base.display(),
            err
        ))
    })?;
    ensure_inside_project(project_root, &canonical)?;
    Ok(canonical)
}

fn ensure_inside_project(project_root: &Path, path: &Path) -> Result<(), CliError> {
    if path.starts_with(project_root) {
        return Ok(());
    }

    Err(validation_error(format!(
        "config bundle path must stay inside the project directory: {}",
        path.display()
    )))
}

fn display_project_path(project_root: &Path, path: &Path) -> String {
    path.strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn validate_environment_name(environment: &str) -> Result<(), CliError> {
    if !environment.is_empty()
        && environment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Ok(());
    }

    Err(validation_error(format!(
        "environment name must contain only letters, digits, '-' or '_': {environment}"
    )))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn is_secret_like_path(path: &str) -> bool {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_ascii_lowercase();

    file_name == ".env"
        || file_name.ends_with(".env")
        || file_name.contains("secret")
        || file_name.contains("password")
        || file_name.contains("private")
        || file_name.ends_with(".key")
}

fn mapping_mut(value: &mut serde_yaml::Value) -> Option<&mut serde_yaml::Mapping> {
    match value {
        serde_yaml::Value::Mapping(mapping) => Some(mapping),
        _ => None,
    }
}

/// Remove `services` entries carrying the `my.stacker.scope: platform` label
/// from a parsed compose document, returning the removed service names.
///
/// Platform-managed services (the nginx-proxy-manager ingress today; see
/// `PLATFORM_MANAGED_APP_CODES`) are installed by their own Ansible role in
/// their own directory, so they must not also appear in the project compose
/// — otherwise the container is deployed twice and the ingress ports collide.
/// User-declared services are labeled `scope: project` (or unlabeled) and are
/// left untouched, so a user's own reverse-proxy service is never dropped.
fn strip_platform_managed_services(compose: &mut serde_yaml::Value) -> Vec<String> {
    let scope_key = serde_yaml::Value::String(crate::helpers::stacker_labels::SCOPE.to_string());
    let Some(services) = mapping_mut(compose)
        .and_then(|root| root.get_mut(serde_yaml::Value::String("services".to_string())))
        .and_then(mapping_mut)
    else {
        return Vec::new();
    };

    let to_remove: Vec<serde_yaml::Value> = services
        .iter()
        .filter(|(_, definition)| {
            definition
                .get("labels")
                .and_then(|labels| labels.as_mapping())
                .and_then(|labels| labels.get(&scope_key))
                .and_then(|scope| scope.as_str())
                .map(|scope| scope == crate::helpers::stacker_labels::SCOPE_PLATFORM)
                .unwrap_or(false)
        })
        .map(|(name, _)| name.clone())
        .collect();

    // Collect the named volumes the doomed services referenced, so we can prune
    // any that become orphaned once those services are gone (e.g. Caddy's
    // caddy_data/caddy_config, which would otherwise linger as unused top-level
    // volume declarations in the remote compose).
    let mut candidate_volumes: Vec<String> = Vec::new();
    for name in &to_remove {
        if let Some(def) = services.get(name) {
            candidate_volumes.extend(named_volume_sources(def));
        }
    }

    let mut removed = Vec::with_capacity(to_remove.len());
    for name in to_remove {
        services.remove(&name);
        if let serde_yaml::Value::String(name) = name {
            removed.push(name);
        }
    }

    // A candidate volume is orphaned only if no *remaining* service still mounts
    // it. Re-borrow services immutably to check, then prune the top-level map.
    if !candidate_volumes.is_empty() {
        let still_referenced: std::collections::HashSet<String> = mapping_mut(compose)
            .and_then(|root| root.get_mut(serde_yaml::Value::String("services".to_string())))
            .and_then(mapping_mut)
            .map(|services| {
                services
                    .values()
                    .flat_map(named_volume_sources)
                    .collect()
            })
            .unwrap_or_default();

        if let Some(volumes) = mapping_mut(compose)
            .and_then(|root| root.get_mut(serde_yaml::Value::String("volumes".to_string())))
            .and_then(mapping_mut)
        {
            for vol in candidate_volumes {
                if !still_referenced.contains(&vol) {
                    volumes.remove(&serde_yaml::Value::String(vol));
                }
            }
        }
    }

    removed
}

/// Extract the *named* volume sources a service mounts (e.g. `caddy_data` from
/// `caddy_data:/data`, or `source: caddy_data` in long syntax). Bind mounts
/// (sources containing `/` or starting with `.`) are host paths, not named
/// volumes, and are ignored.
fn named_volume_sources(service_def: &serde_yaml::Value) -> Vec<String> {
    let Some(serde_yaml::Value::Sequence(volumes)) = service_def.get("volumes") else {
        return Vec::new();
    };
    let is_named = |src: &str| !src.is_empty() && !src.contains('/') && !src.starts_with('.');
    volumes
        .iter()
        .filter_map(|vol| match vol {
            // Short syntax: "name:/container/path[:opts]"
            serde_yaml::Value::String(s) => {
                let src = s.split(':').next().unwrap_or("");
                is_named(src).then(|| src.to_string())
            }
            // Long syntax: { type: volume, source: name, target: ... }
            serde_yaml::Value::Mapping(_) => vol
                .get("source")
                .and_then(|s| s.as_str())
                .filter(|src| is_named(src))
                .map(|src| src.to_string()),
            _ => None,
        })
        .collect()
}

fn validation_error(message: impl Into<String>) -> CliError {
    CliError::ConfigValidation(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn build_config_bundle_collects_env_file_and_file_mounts_for_environment() {
        let dir = TempDir::new().unwrap();
        let compose_dir = dir.path().join("docker/production");
        std::fs::create_dir_all(&compose_dir).unwrap();
        std::fs::write(compose_dir.join(".env"), "RUST_LOG=warning\n").unwrap();
        std::fs::write(compose_dir.join("nginx.conf"), "events {}\n").unwrap();
        std::fs::write(
            compose_dir.join("compose.yml"),
            r#"
services:
  api:
    image: device-api:latest
    env_file:
      - .env
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
"#,
        )
        .unwrap();

        let artifacts = build_config_bundle(
            dir.path(),
            "production",
            &compose_dir.join("compose.yml"),
            Some(&compose_dir.join(".env")),
            &compose_dir,
            false,
        )
        .expect("bundle should be built");

        assert_eq!(artifacts.environment, "production");
        assert!(artifacts
            .manifest_path
            .ends_with(".stacker/deploy/production/config-bundle.manifest.json"));
        assert!(artifacts
            .archive_path
            .ends_with(".stacker/deploy/production/config-bundle.tar.zst"));
        assert!(artifacts
            .remote_compose_path
            .ends_with(".stacker/deploy/production/docker-compose.remote.yml"));
        assert!(artifacts.manifest_path.exists());
        assert!(artifacts.archive_path.exists());
        assert!(artifacts.remote_compose_path.exists());

        let sources: Vec<&str> = artifacts
            .manifest
            .files
            .iter()
            .map(|file| file.source_path.as_str())
            .collect();
        assert!(sources.contains(&"docker/production/.env"));
        assert!(sources.contains(&"docker/production/nginx.conf"));

        let remote_compose = std::fs::read_to_string(&artifacts.remote_compose_path).unwrap();
        assert!(remote_compose.contains("docker/production/.env"));
        assert!(remote_compose.contains("docker/production/nginx.conf:/etc/nginx/nginx.conf:ro"));

        let names: Vec<&str> = artifacts
            .config_files
            .iter()
            .filter_map(|file| file.get("name").and_then(|name| name.as_str()))
            .collect();
        assert!(names.contains(&"docker-compose.yml"));
        assert!(names.contains(&".env"));
        assert!(names.contains(&"nginx.conf"));

        let root_env = artifacts
            .config_files
            .iter()
            .find(|file| {
                file.get("destination_path")
                    .and_then(|value| value.as_str())
                    == Some(".env")
            })
            .expect("selected env file should also be uploaded as compose root .env");
        assert_eq!(root_env["content"], "RUST_LOG=warning\n");
    }

    #[test]
    fn build_config_bundle_keeps_root_compose_env_file_project_relative() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".env"), "APP_ENV=production\n").unwrap();
        std::fs::write(
            dir.path().join("docker-compose.yml"),
            r#"
services:
  web:
    image: nginx:latest
    env_file:
      - .env
"#,
        )
        .unwrap();

        let artifacts = build_config_bundle(
            dir.path(),
            "production",
            &dir.path().join("docker-compose.yml"),
            None,
            dir.path(),
            false,
        )
        .expect("bundle should be built");

        let remote_compose = std::fs::read_to_string(&artifacts.remote_compose_path).unwrap();
        assert!(remote_compose.contains(".env"));
        assert!(!remote_compose.contains("/opt/stacker/deployments"));

        assert!(artifacts.config_files.iter().any(|file| {
            file.get("destination_path")
                .and_then(|value| value.as_str())
                == Some(".env")
        }));
    }

    #[test]
    fn build_config_bundle_resolves_generated_compose_refs_against_project_root() {
        // A generated compose lives under .stacker/ but its bind-mount paths are
        // authored relative to the project root (from stacker.yml app.volumes).
        // With reference_base = project root, "./config.yaml" must resolve to
        // <project>/config.yaml, not <project>/.stacker/config.yaml.
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("config.yaml"), "key: value\n").unwrap();
        let stacker_dir = dir.path().join(".stacker");
        std::fs::create_dir_all(&stacker_dir).unwrap();
        std::fs::write(
            stacker_dir.join("docker-compose.yml"),
            r#"
services:
  app:
    image: example/app:latest
    volumes:
      - ./config.yaml:/app/config.yaml:ro
"#,
        )
        .unwrap();

        let artifacts = build_config_bundle(
            dir.path(),
            "default",
            &stacker_dir.join("docker-compose.yml"),
            None,
            dir.path(),
            false,
        )
        .expect("bundle should be built for generated compose");

        let sources: Vec<&str> = artifacts
            .manifest
            .files
            .iter()
            .map(|file| file.source_path.as_str())
            .collect();
        assert!(
            sources.contains(&"config.yaml"),
            "expected project-root config.yaml, got {sources:?}"
        );

        let remote_compose = std::fs::read_to_string(&artifacts.remote_compose_path).unwrap();
        assert!(remote_compose.contains("config.yaml:/app/config.yaml:ro"));
    }

    #[test]
    fn validate_relative_destinations_rejects_absolute_paths() {
        let err = validate_relative_destinations(&[ConfigBundleFile {
            source_path: ".env".to_string(),
            destination_path: "/opt/stacker/deployments/production/files/.env".to_string(),
            mode: "0644".to_string(),
            size: 12,
            sha256: "abc".to_string(),
        }])
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("config bundle destination must be project-relative"));
    }

    #[test]
    fn build_config_bundle_passes_through_directory_and_missing_mounts() {
        // romm-style app: data directories (existing + not-yet-created) and a
        // real config file mounted alongside. Directory/missing mounts must be
        // left untouched (created on the target), while the file is bundled.
        let dir = TempDir::new().unwrap();
        let compose_dir = dir.path().join("docker/production");
        std::fs::create_dir_all(compose_dir.join("library")).unwrap();
        std::fs::write(compose_dir.join("romm.env"), "ROMM_DB=romm\n").unwrap();
        std::fs::write(
            compose_dir.join("compose.yml"),
            r#"
services:
  romm:
    image: rommapp/romm:latest
    volumes:
      - ./library:/romm/library
      - ./assets:/romm/assets:rw
      - ./romm.env:/romm/config/romm.env:ro
"#,
        )
        .unwrap();

        let artifacts = build_config_bundle(
            dir.path(),
            "production",
            &compose_dir.join("compose.yml"),
            None,
            &compose_dir,
            false,
        )
        .expect("directory mounts should not block the bundle");

        // Only the real file is collected.
        let sources: Vec<&str> = artifacts
            .manifest
            .files
            .iter()
            .map(|file| file.source_path.as_str())
            .collect();
        assert_eq!(sources, vec!["docker/production/romm.env"]);

        // Directory + missing mounts pass through verbatim; the file is rewritten.
        let remote = std::fs::read_to_string(&artifacts.remote_compose_path).unwrap();
        assert!(remote.contains("./library:/romm/library"));
        assert!(remote.contains("./assets:/romm/assets:rw"));
        assert!(remote.contains("docker/production/romm.env:/romm/config/romm.env:ro"));
    }

    #[test]
    fn build_config_bundle_attaches_agent_network_when_requested() {
        let dir = TempDir::new().unwrap();
        let compose_dir = dir.path().join("docker/production");
        std::fs::create_dir_all(&compose_dir).unwrap();
        std::fs::write(
            compose_dir.join("compose.yml"),
            r#"
services:
  app:
    image: example/app:latest
  db:
    image: mysql:8
"#,
        )
        .unwrap();

        let artifacts = build_config_bundle(
            dir.path(),
            "production",
            &compose_dir.join("compose.yml"),
            None,
            &compose_dir,
            true,
        )
        .expect("bundle should be built");

        let remote: serde_yaml::Value =
            serde_yaml::from_str(&std::fs::read_to_string(&artifacts.remote_compose_path).unwrap())
                .unwrap();
        // Every service joins default_network, declared external at the top level.
        for svc in ["app", "db"] {
            let nets = remote["services"][svc]["networks"].as_sequence().unwrap();
            assert!(
                nets.iter().any(|n| n.as_str() == Some("default_network")),
                "service {svc} should join default_network"
            );
        }
        assert_eq!(remote["networks"]["default_network"]["external"], true);
    }

    #[test]
    fn build_config_bundle_reports_missing_env_file_path() {
        let dir = TempDir::new().unwrap();
        let compose_dir = dir.path().join("docker/production");
        std::fs::create_dir_all(&compose_dir).unwrap();
        std::fs::write(
            compose_dir.join("compose.yml"),
            r#"
services:
  upload:
    image: syncopia/upload:latest
    env_file:
      - upload.env
"#,
        )
        .unwrap();

        let err = build_config_bundle(
            dir.path(),
            "production",
            &compose_dir.join("compose.yml"),
            None,
            &compose_dir,
            false,
        )
        .unwrap_err();

        assert!(
            err.to_string().contains("docker/production/upload.env")
                || err.to_string().contains("docker/production\\upload.env"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn artifact_metadata_marks_secret_like_files_hidden() {
        let manifest = ConfigBundleManifest {
            version: 1,
            environment: "production".to_string(),
            files: vec![
                ConfigBundleFile {
                    source_path: "docker/production/.env".to_string(),
                    destination_path: "docker/production/.env".to_string(),
                    mode: "0644".to_string(),
                    size: 12,
                    sha256: "abc".to_string(),
                },
                ConfigBundleFile {
                    source_path: "docker/production/nginx.conf".to_string(),
                    destination_path: "docker/production/nginx.conf".to_string(),
                    mode: "0644".to_string(),
                    size: 10,
                    sha256: "def".to_string(),
                },
            ],
        };
        let artifacts = ConfigBundleArtifacts {
            environment: "production".to_string(),
            manifest_path: PathBuf::from(".stacker/deploy/production/config-bundle.manifest.json"),
            archive_path: PathBuf::from(".stacker/deploy/production/config-bundle.tar.zst"),
            remote_compose_path: PathBuf::from(
                ".stacker/deploy/production/docker-compose.remote.yml",
            ),
            manifest,
            config_files: vec![],
            synthesized_environment: false,
        };

        let metadata = artifacts.artifact_metadata();
        assert_eq!(metadata["environment"], "production");
        assert_eq!(metadata["config_files"][0]["content_hidden"], true);
        assert_eq!(metadata["config_files"][1]["content_hidden"], false);
        assert!(metadata["config_files"][0].get("content").is_none());
    }

    #[test]
    fn build_config_bundle_strips_platform_managed_services_from_remote_compose() {
        // A `proxy: type: nginx-proxy-manager` deploy synthesizes a
        // platform-scoped `proxy-manager` service into the compose. The
        // install-service deploys NPM separately via its own role, so the
        // remote bundle must NOT also carry it (double-deploy / port collision).
        // A user's own reverse-proxy declared as a normal service is
        // project-scoped and must be preserved.
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("docker-compose.yml"),
            r#"
services:
  app:
    image: myapp:latest
    labels:
      my.stacker.scope: "project"
  proxy-manager:
    image: jc21/nginx-proxy-manager:latest
    ports:
      - "80:80"
      - "443:443"
      - "81:81"
    labels:
      my.stacker.scope: "platform"
      my.stacker.service: "nginx_proxy_manager"
  my-own-traefik:
    image: traefik:v2.10
    labels:
      my.stacker.scope: "project"
"#,
        )
        .unwrap();

        let artifacts = build_config_bundle(
            dir.path(),
            "default",
            &dir.path().join("docker-compose.yml"),
            None,
            dir.path(),
            false,
        )
        .expect("bundle should be built");

        let compose_content = artifacts
            .config_files
            .iter()
            .find(|file| file["name"] == "docker-compose.yml")
            .and_then(|file| file["content"].as_str())
            .expect("remote bundle contains the compose file");

        // Assert on the scope *label* (the contract), not the generated
        // service name: no `scope: platform` service may survive in the
        // remote compose, while project-scoped services are preserved.
        let parsed: serde_yaml::Value =
            serde_yaml::from_str(compose_content).expect("remote compose is valid yaml");
        let services = parsed
            .get("services")
            .and_then(|services| services.as_mapping())
            .expect("remote compose has a services map");

        let scope_key =
            serde_yaml::Value::String(crate::helpers::stacker_labels::SCOPE.to_string());
        let service_scope = |name: &str| -> Option<String> {
            services
                .get(name)?
                .get("labels")?
                .as_mapping()?
                .get(&scope_key)?
                .as_str()
                .map(str::to_string)
        };

        // No platform-scoped service remains anywhere in the shipped compose …
        let has_platform_scoped = services.values().any(|definition| {
            definition
                .get("labels")
                .and_then(|labels| labels.as_mapping())
                .and_then(|labels| labels.get(&scope_key))
                .and_then(|scope| scope.as_str())
                == Some(crate::helpers::stacker_labels::SCOPE_PLATFORM)
        });
        assert!(
            !has_platform_scoped,
            "no `scope: platform` service should survive in the remote compose:\n{compose_content}"
        );

        // … while the app and the user's own project-scoped proxy are kept.
        assert_eq!(
            service_scope("app").as_deref(),
            Some(crate::helpers::stacker_labels::SCOPE_PROJECT)
        );
        assert_eq!(
            service_scope("my-own-traefik").as_deref(),
            Some(crate::helpers::stacker_labels::SCOPE_PROJECT),
            "a user's own project-scoped proxy must not be stripped:\n{compose_content}"
        );
    }

    #[test]
    fn strip_platform_managed_services_prunes_orphaned_named_volumes() {
        // Stripping the platform proxy must also drop the named volumes only it
        // used (Caddy's caddy_data/caddy_config), while keeping volumes still
        // referenced by a surviving service and untouched bind mounts.
        let mut compose: serde_yaml::Value = serde_yaml::from_str(
            r#"
services:
  app:
    image: myapp:latest
    volumes:
      - app_data:/data
    labels:
      my.stacker.scope: "project"
  caddy:
    image: caddy:2-alpine
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy_data:/data
      - caddy_config:/config
      - app_data:/shared
    labels:
      my.stacker.scope: "platform"
volumes:
  app_data: {}
  caddy_data: {}
  caddy_config: {}
"#,
        )
        .unwrap();

        let removed = strip_platform_managed_services(&mut compose);
        assert_eq!(removed, vec!["caddy".to_string()]);

        let volumes = compose
            .get("volumes")
            .and_then(|v| v.as_mapping())
            .expect("top-level volumes map");
        let has = |name: &str| volumes.contains_key(serde_yaml::Value::String(name.to_string()));

        // Orphaned (only caddy used them) → pruned.
        assert!(!has("caddy_data"), "caddy_data should be pruned");
        assert!(!has("caddy_config"), "caddy_config should be pruned");
        // Still used by the surviving `app` service → kept.
        assert!(has("app_data"), "app_data is still referenced and must stay");
    }
}
