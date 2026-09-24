//! Steps for `tests/features/bake_sanitization.feature`.
//!
//! These exercise the pure policy functions directly — no HTTP, no database.
//! The SSH orchestration around them (`finalize_build_box`) needs a real build
//! box and is out of scope here; what is covered is the part that decides *what*
//! gets removed, which is where a mistake destroys an image or leaks a secret.

use cucumber::{given, then, when};
use std::collections::{BTreeMap, BTreeSet};

use stacker::cli::generator::compose::parameterize_embedded_secret_values;
use stacker::helpers::bake_finalize::{scrub_env_file, volume_reset_commands};

use super::StepWorld;

// ─── Given ───────────────────────────────────────────────────────

#[given(regex = r#"^the contract declares "([^"]*)" as generated$"#)]
async fn given_contract_declares(world: &mut StepWorld, key: String) {
    world.bake.protected.insert(key);
}

#[given(regex = r#"^the contract declares nothing$"#)]
async fn given_contract_empty(world: &mut StepWorld) {
    world.bake.protected.clear();
}

#[given(regex = r#"^the build box \.env contains:$"#)]
async fn given_env_contents(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let body = step.docstring().cloned().unwrap_or_default();
    world.bake.env_content = format!("{}\n", body.trim_matches('\n'));
}

#[given(regex = r#"^"([^"]*)" on the build box resolves to "([^"]*)"$"#)]
async fn given_env_value(world: &mut StepWorld, key: String, value: String) {
    world.bake.env_values.insert(key, value);
}

#[given(regex = r#"^the generated compose is:$"#)]
async fn given_compose(world: &mut StepWorld, step: &cucumber::gherkin::Step) {
    let body = step.docstring().cloned().unwrap_or_default();
    world.bake.compose = format!("{}\n", body.trim_matches('\n'));
}

#[given(regex = r#"^the stack keeps the volume matching "([^"]*)"$"#)]
async fn given_keep_volume(world: &mut StepWorld, name: String) {
    world.bake.keep_volumes.push(name);
}

// ─── When ────────────────────────────────────────────────────────

#[when(regex = r#"^the \.env is scrubbed for the snapshot$"#)]
async fn when_scrub_env(world: &mut StepWorld) {
    world.bake.scrubbed_env = scrub_env_file(&world.bake.env_content, &world.bake.protected);
}

#[when(regex = r#"^the compose is sanitized for the snapshot$"#)]
async fn when_sanitize_compose(world: &mut StepWorld) {
    match parameterize_embedded_secret_values(
        &world.bake.compose,
        &world.bake.env_values,
        &world.bake.protected,
    ) {
        Ok(out) => {
            world.bake.sanitized_compose = Some(out);
            world.bake.sanitize_error = None;
        }
        Err(conflict) => {
            world.bake.sanitized_compose = None;
            world.bake.sanitize_error = Some(conflict.keys);
        }
    }
}

#[when(regex = r#"^the volume reset commands are built for "([^"]*)"$"#)]
async fn when_build_volume_commands(world: &mut StepWorld, project_dir: String) {
    let keep: Vec<&str> = world.bake.keep_volumes.iter().map(String::as_str).collect();
    world.bake.volume_commands = volume_reset_commands(&project_dir, &keep)
        .expect("valid keep list")
        .join(" ; ");
}

// ─── Then: .env ──────────────────────────────────────────────────

#[then(regex = r#"^the scrubbed \.env has "([^"]*)" blanked$"#)]
async fn then_env_blanked(world: &mut StepWorld, key: String) {
    let expected = format!("{key}=\n");
    assert!(
        world.bake.scrubbed_env.contains(&expected),
        "expected `{key}` blanked in:\n{}",
        world.bake.scrubbed_env
    );
}

#[then(regex = r#"^the scrubbed \.env still has "([^"]*)" set to "([^"]*)"$"#)]
async fn then_env_kept(world: &mut StepWorld, key: String, value: String) {
    let expected = format!("{key}={value}\n");
    assert!(
        world.bake.scrubbed_env.contains(&expected),
        "expected `{key}={value}` preserved in:\n{}",
        world.bake.scrubbed_env
    );
}

#[then(regex = r#"^the scrubbed \.env contains no occurrence of "([^"]*)"$"#)]
async fn then_env_has_no_literal(world: &mut StepWorld, literal: String) {
    assert!(
        !world.bake.scrubbed_env.contains(&literal),
        "literal still present in:\n{}",
        world.bake.scrubbed_env
    );
}

#[then(regex = r#"^the scrubbed \.env is unchanged$"#)]
async fn then_env_unchanged(world: &mut StepWorld) {
    assert_eq!(
        world.bake.scrubbed_env, world.bake.env_content,
        "nothing is declared, so nothing may be blanked"
    );
}

// ─── Then: compose ───────────────────────────────────────────────

#[then(regex = r#"^the sanitized compose contains "([^"]*)"$"#)]
async fn then_compose_contains(world: &mut StepWorld, needle: String) {
    let out = world
        .bake
        .sanitized_compose
        .as_ref()
        .expect("sanitizing should have succeeded");
    assert!(out.contains(&needle), "expected `{needle}` in:\n{out}");
}

#[then(regex = r#"^the sanitized compose contains no occurrence of "([^"]*)"$"#)]
async fn then_compose_has_no_literal(world: &mut StepWorld, literal: String) {
    let out = world
        .bake
        .sanitized_compose
        .as_ref()
        .expect("sanitizing should have succeeded");
    assert!(!out.contains(&literal), "literal still present in:\n{out}");
}

#[then(regex = r#"^sanitizing fails naming both "([^"]*)" and "([^"]*)"$"#)]
async fn then_sanitize_conflict(world: &mut StepWorld, first: String, second: String) {
    let keys = world
        .bake
        .sanitize_error
        .as_ref()
        .expect("sanitizing should have been refused");
    assert!(
        keys.contains(&first) && keys.contains(&second),
        "expected both `{first}` and `{second}` in the conflict, got {keys:?}"
    );
}

// ─── Then: volumes ───────────────────────────────────────────────

#[then(regex = r#"^the commands list volumes from the project compose$"#)]
async fn then_volumes_from_project(world: &mut StepWorld) {
    assert!(
        world
            .bake
            .volume_commands
            .contains("docker compose config --volumes"),
        "commands: {}",
        world.bake.volume_commands
    );
}

#[then(regex = r#"^the commands scope removal by the compose volume label$"#)]
async fn then_volumes_scoped_by_label(world: &mut StepWorld) {
    assert!(
        world
            .bake
            .volume_commands
            .contains("--filter label=com.docker.compose.volume="),
        "commands: {}",
        world.bake.volume_commands
    );
}

#[then(regex = r#"^the commands never list every volume on the host$"#)]
async fn then_volumes_not_host_wide(world: &mut StepWorld) {
    assert!(
        !world.bake.volume_commands.contains("docker volume ls -q |"),
        "an unfiltered host-wide listing would delete the ingress' certificates \
         and abort the bake on the first in-use volume; commands: {}",
        world.bake.volume_commands
    );
}

#[then(regex = r#"^the commands skip the volume matching "([^"]*)"$"#)]
async fn then_volumes_skip_kept(world: &mut StepWorld, name: String) {
    let expected = format!("case \"$v\" in {name})");
    assert!(
        world.bake.volume_commands.contains(&expected),
        "expected `{expected}`; commands: {}",
        world.bake.volume_commands
    );
}

/// Scratch state for the bake-sanitization scenarios.
#[derive(Debug, Default)]
pub struct BakeWorld {
    pub protected: BTreeSet<String>,
    pub env_values: BTreeMap<String, String>,
    pub env_content: String,
    pub scrubbed_env: String,
    pub compose: String,
    pub sanitized_compose: Option<String>,
    pub sanitize_error: Option<Vec<String>>,
    pub keep_volumes: Vec<String>,
    pub volume_commands: String,
    pub resolved_compose: String,
    pub unfillable: Vec<String>,
    pub required: Vec<String>,
    pub identity_commands: String,
    pub allow_unsanitized: bool,
    pub refusal: Option<String>,
    pub warning: Option<String>,
    pub file_content: String,
    pub write_commands: Vec<String>,
    pub stages: Vec<stacker::helpers::bake_finalize::FinalizeStage>,
    pub advice: String,
    pub lost: Vec<String>,
    pub declared_volumes: Vec<(String, String)>,
    pub declared_fields: Vec<(String, String)>,
    pub kept: Vec<String>,
}

// ─── references that survive into the baked image ────────────────

#[when(regex = r#"^references outside the contract are resolved$"#)]
async fn when_resolve_references(world: &mut StepWorld) {
    let (out, unresolved) = stacker::cli::generator::compose::resolve_non_contract_references(
        &world.bake.compose,
        &world.bake.env_values,
        &world.bake.protected,
    );
    world.bake.resolved_compose = out;
    world.bake.unfillable = unresolved.into_iter().map(|r| r.name).collect();
}

#[when(regex = r#"^the required environment keys are collected$"#)]
async fn when_collect_required(world: &mut StepWorld) {
    world.bake.required = stacker::cli::generator::compose::required_env_keys(
        &world.bake.compose,
        &world.bake.protected,
    )
    .into_iter()
    .collect();
}

#[then(regex = r#"^the resolved compose contains "([^"]*)"$"#)]
async fn then_resolved_contains(world: &mut StepWorld, needle: String) {
    assert!(
        world.bake.resolved_compose.contains(&needle),
        "expected `{needle}` in:\n{}",
        world.bake.resolved_compose
    );
}

#[then(regex = r#"^the resolved compose contains no occurrence of "([^"]*)"$"#)]
async fn then_resolved_lacks(world: &mut StepWorld, literal: String) {
    assert!(
        !world.bake.resolved_compose.contains(&literal),
        "literal still present in:\n{}",
        world.bake.resolved_compose
    );
}

#[then(regex = r#"^no reference is reported as unfillable$"#)]
async fn then_nothing_unfillable(world: &mut StepWorld) {
    assert!(
        world.bake.unfillable.is_empty(),
        "unexpected: {:?}",
        world.bake.unfillable
    );
}

#[then(regex = r#"^"([^"]*)" is reported as unfillable$"#)]
async fn then_reported_unfillable(world: &mut StepWorld, name: String) {
    assert!(
        world.bake.unfillable.contains(&name),
        "expected `{name}` among {:?}",
        world.bake.unfillable
    );
}

#[then(regex = r#"^the required keys are exactly "([^"]*)"$"#)]
async fn then_required_exactly(world: &mut StepWorld, csv: String) {
    let expected: Vec<String> = csv.split(',').map(|s| s.trim().to_string()).collect();
    assert_eq!(world.bake.required, expected);
}

// ─── access belonging to the author ──────────────────────────────

#[when(regex = r#"^the identity reset commands are built$"#)]
async fn when_build_identity_commands(world: &mut StepWorld) {
    world.bake.identity_commands =
        stacker::helpers::bake_finalize::identity_reset_commands().join(" ; ");
}

#[then(regex = r#"^the commands remove "([^"]*)"$"#)]
async fn then_commands_remove(world: &mut StepWorld, path: String) {
    assert!(
        world.bake.identity_commands.contains(&path),
        "expected `{path}` to be removed; commands: {}",
        world.bake.identity_commands
    );
}

// ─── a bake that cannot sanitize ─────────────────────────────────

#[given(regex = r#"^unsanitized snapshots are explicitly allowed$"#)]
async fn given_allow_unsanitized(world: &mut StepWorld) {
    world.bake.allow_unsanitized = true;
}

#[given(regex = r#"^the build box has no \.env beside the compose$"#)]
async fn given_no_env_file(world: &mut StepWorld) {
    world.bake.env_values.clear();
}

#[when(regex = r#"^the bake checks whether it can sanitize$"#)]
async fn when_check_contract(world: &mut StepWorld) {
    world.bake.refusal = stacker::helpers::bake_finalize::check_contract_usable(
        &world.bake.protected,
        world.bake.allow_unsanitized,
    )
    .err()
    .map(|e| e.to_string());
}

#[when(regex = r#"^the bake checks the values it has to work from$"#)]
async fn when_check_env_values(world: &mut StepWorld) {
    world.bake.refusal = None;
    world.bake.warning = stacker::helpers::bake_finalize::env_scan_warning(
        &world.bake.env_values,
        &world.bake.protected,
    );
}

#[then(regex = r#"^the bake is refused$"#)]
async fn then_bake_refused(world: &mut StepWorld) {
    assert!(
        world.bake.refusal.is_some(),
        "the bake should not have been allowed to proceed"
    );
}

#[then(regex = r#"^the bake is allowed$"#)]
async fn then_bake_allowed(world: &mut StepWorld) {
    assert!(
        world.bake.refusal.is_none(),
        "unexpected refusal: {:?}",
        world.bake.refusal
    );
}

#[then(regex = r#"^the refusal mentions "([^"]*)"$"#)]
async fn then_refusal_mentions(world: &mut StepWorld, needle: String) {
    let message = world
        .bake
        .refusal
        .as_ref()
        .expect("there should be a refusal");
    assert!(
        message.contains(&needle),
        "expected `{needle}` in: {message}"
    );
}

#[then(regex = r#"^a warning mentions "([^"]*)"$"#)]
async fn then_warning_mentions(world: &mut StepWorld, needle: String) {
    let warning = world
        .bake
        .warning
        .as_ref()
        .expect("a warning should have been raised");
    assert!(
        warning.contains(&needle),
        "expected `{needle}` in: {warning}"
    );
}

#[then(regex = r#"^no warning is raised$"#)]
async fn then_no_warning(world: &mut StepWorld) {
    assert!(
        world.bake.warning.is_none(),
        "unexpected warning: {:?}",
        world.bake.warning
    );
}

// ─── writing a file to the build box ─────────────────────────────

#[when(regex = r#"^a file of (\d+) bytes is written to the build box$"#)]
async fn when_write_file(world: &mut StepWorld, size: usize) {
    // Varied content, so a seam corrupted by bad chunking is detectable.
    world.bake.file_content = (0..size).map(|i| ((i % 26) as u8 + b'a') as char).collect();
    world.bake.write_commands =
        stacker::helpers::bake_finalize::write_file_commands("/tmp/x", &world.bake.file_content);
}

#[then(regex = r#"^it takes (\d+) command$"#)]
async fn then_command_count(world: &mut StepWorld, expected: usize) {
    assert_eq!(world.bake.write_commands.len(), expected);
}

#[then(regex = r#"^it takes more than one command$"#)]
async fn then_more_than_one(world: &mut StepWorld) {
    assert!(
        world.bake.write_commands.len() > 1,
        "expected chunking, got {}",
        world.bake.write_commands.len()
    );
}

#[then(regex = r#"^the first command truncates the file$"#)]
async fn then_first_truncates(world: &mut StepWorld) {
    let first = &world.bake.write_commands[0];
    assert!(first.contains("> /tmp/x"), "not a write: {first}");
    assert!(!first.contains(">> /tmp/x"), "must not append: {first}");
}

#[then(regex = r#"^every later command appends$"#)]
async fn then_rest_append(world: &mut StepWorld) {
    for cmd in &world.bake.write_commands[1..] {
        assert!(cmd.contains(">> /tmp/x"), "chunk must append: {cmd}");
    }
}

#[then(regex = r#"^every command fits in a single argument$"#)]
async fn then_commands_fit(world: &mut StepWorld) {
    for cmd in &world.bake.write_commands {
        assert!(
            cmd.len() < 128 * 1024,
            "a command of {} chars would be rejected as too long",
            cmd.len()
        );
    }
}

#[then(regex = r#"^decoding the chunks in order yields the original content$"#)]
async fn then_chunks_round_trip(world: &mut StepWorld) {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    let mut rebuilt = Vec::new();
    for cmd in &world.bake.write_commands {
        let encoded = cmd.split_whitespace().nth(2).expect("printf %s <payload>");
        rebuilt.extend(STANDARD.decode(encoded).expect("each chunk decodes alone"));
    }

    assert_eq!(
        String::from_utf8(rebuilt).expect("valid utf-8"),
        world.bake.file_content
    );
}

// ─── what a failed finalize reports ──────────────────────────────

#[given(regex = r#"^the finalize completed "([^"]*)"$"#)]
async fn given_stages_completed(world: &mut StepWorld, csv: String) {
    use stacker::helpers::bake_finalize::FinalizeStage;

    world.bake.stages = csv
        .split(',')
        .map(|s| match s.trim() {
            "read" => FinalizeStage::Read,
            "teardown" => FinalizeStage::Teardown,
            "rewrite compose" => FinalizeStage::RewriteCompose,
            "clear env" => FinalizeStage::ClearEnv,
            "strip identity" => FinalizeStage::StripIdentity,
            other => panic!("unknown finalize stage: {other}"),
        })
        .collect();
}

#[when(regex = r#"^the recovery advice is produced$"#)]
async fn when_recovery_advice(world: &mut StepWorld) {
    world.bake.advice = stacker::helpers::bake_finalize::recovery_advice(&world.bake.stages);
}

#[then(regex = r#"^the advice says the bake can be retried$"#)]
async fn then_advice_retry(world: &mut StepWorld) {
    assert!(
        world.bake.advice.contains("can be retried"),
        "advice: {}",
        world.bake.advice
    );
}

#[then(regex = r#"^the advice asks for a fresh build box$"#)]
async fn then_advice_fresh_box(world: &mut StepWorld) {
    assert!(
        world.bake.advice.contains("fresh build box"),
        "advice: {}",
        world.bake.advice
    );
}

#[then(regex = r#"^the advice does not ask for a fresh build box$"#)]
async fn then_advice_no_fresh_box(world: &mut StepWorld) {
    assert!(
        !world.bake.advice.contains("fresh build box"),
        "advice: {}",
        world.bake.advice
    );
}

#[then(regex = r#"^the advice mentions "([^"]*)"$"#)]
async fn then_advice_mentions(world: &mut StepWorld, needle: String) {
    assert!(
        world.bake.advice.contains(&needle),
        "expected `{needle}` in: {}",
        world.bake.advice
    );
}

// ─── values a clone would lose, and the list form ────────────────

#[given(regex = r#"^"([^"]*)" is a protected compose key$"#)]
async fn given_protected_compose_key(world: &mut StepWorld, key: String) {
    world.bake.protected.insert(key);
}

#[when(regex = r#"^the bake checks what a clone would lose$"#)]
async fn when_check_lost(world: &mut StepWorld) {
    world.bake.lost = stacker::helpers::bake_finalize::env_file_values_lost_on_clone(
        &world.bake.compose,
        &world.bake.env_values,
        &world.bake.protected,
    );
}

#[when(regex = r#"^whole-value keys are parameterized$"#)]
async fn when_parameterize_whole_values(world: &mut StepWorld) {
    let keys: std::collections::HashSet<String> = world.bake.protected.iter().cloned().collect();
    world.bake.resolved_compose =
        stacker::cli::generator::compose::parameterize_compose_env_vars(&world.bake.compose, &keys);
}

#[then(regex = r#"^"([^"]*)" is reported as lost$"#)]
async fn then_reported_lost(world: &mut StepWorld, key: String) {
    assert!(
        world.bake.lost.contains(&key),
        "expected `{key}` among {:?}",
        world.bake.lost
    );
}

#[then(regex = r#"^"([^"]*)" is not reported as lost$"#)]
async fn then_not_reported_lost(world: &mut StepWorld, key: String) {
    assert!(
        !world.bake.lost.contains(&key),
        "`{key}` should survive; got {:?}",
        world.bake.lost
    );
}

#[then(regex = r#"^nothing is reported as lost$"#)]
async fn then_nothing_lost(world: &mut StepWorld) {
    assert!(
        world.bake.lost.is_empty(),
        "unexpected: {:?}",
        world.bake.lost
    );
}

#[then(regex = r#"^the parameterized compose contains "([^"]*)"$"#)]
async fn then_parameterized_contains(world: &mut StepWorld, needle: String) {
    assert!(
        world.bake.resolved_compose.contains(&needle),
        "expected `{needle}` in:\n{}",
        world.bake.resolved_compose
    );
}

#[when(regex = r#"^the volume reset commands are built and may fail$"#)]
async fn when_build_volume_commands_fallible(world: &mut StepWorld) {
    let keep: Vec<&str> = world.bake.keep_volumes.iter().map(String::as_str).collect();
    world.bake.refusal =
        stacker::helpers::bake_finalize::volume_reset_commands("/home/trydirect/project", &keep)
            .err()
            .map(|e| e.to_string());
}

#[then(regex = r#"^building the commands is refused$"#)]
async fn then_build_refused(world: &mut StepWorld) {
    assert!(
        world.bake.refusal.is_some(),
        "a keep entry with shell syntax must not be turned into a pattern"
    );
}

#[then(regex = r#"^the commands do not keep every name containing "([^"]*)"$"#)]
async fn then_not_bare_substring(world: &mut StepWorld, name: String) {
    let bare = format!("*{name}*");
    assert!(
        !world.bake.volume_commands.contains(&bare),
        "a bare substring match would also keep `not-{name}-backup`: {}",
        world.bake.volume_commands
    );
}

// ─── author-declared volume policy ───────────────────────────────

#[given(regex = r#"^the contract declares volume "([^"]*)" on service "([^"]*)" as fixed$"#)]
async fn given_fixed_volume(world: &mut StepWorld, volume: String, service: String) {
    world.bake.declared_volumes.push((service, volume));
}

#[given(regex = r#"^service "([^"]*)" regenerates "([^"]*)"$"#)]
async fn given_service_regenerates(world: &mut StepWorld, service: String, field: String) {
    world.bake.declared_fields.push((service, field));
}

fn build_contract(world: &StepWorld) -> stacker::cli::config_parser::ConfigContract {
    let mut services = serde_json::Map::new();
    for (service, volume) in &world.bake.declared_volumes {
        let entry = services
            .entry(service.clone())
            .or_insert_with(|| serde_json::json!({}));
        entry["volumes"][volume] = serde_json::json!({ "mutability": "fixed" });
    }
    for (service, field) in &world.bake.declared_fields {
        let entry = services
            .entry(service.clone())
            .or_insert_with(|| serde_json::json!({}));
        entry["fields"][field] =
            serde_json::json!({ "mutability": "generated", "type": "alphanumeric" });
    }
    serde_json::from_value(serde_json::json!({ "services": services })).expect("contract parses")
}

#[when(regex = r#"^the kept volumes are collected$"#)]
async fn when_collect_kept(world: &mut StepWorld) {
    let contract = build_contract(world);
    world.bake.kept = stacker::helpers::bake_finalize::volumes_to_keep(&contract);
}

#[when(regex = r#"^the declaration is checked$"#)]
async fn when_check_declaration(world: &mut StepWorld) {
    let contract = build_contract(world);
    world.bake.refusal = stacker::helpers::bake_finalize::check_volume_declarations(&contract)
        .err()
        .map(|e| e.to_string());
}

#[then(regex = r#"^"([^"]*)" is kept$"#)]
async fn then_volume_kept(world: &mut StepWorld, name: String) {
    assert!(
        world.bake.kept.contains(&name),
        "expected `{name}` among {:?}",
        world.bake.kept
    );
}

#[then(regex = r#"^nothing is kept$"#)]
async fn then_nothing_kept(world: &mut StepWorld) {
    assert!(
        world.bake.kept.is_empty(),
        "unexpected: {:?}",
        world.bake.kept
    );
}

#[then(regex = r#"^the declaration is refused$"#)]
async fn then_declaration_refused(world: &mut StepWorld) {
    assert!(
        world.bake.refusal.is_some(),
        "a volume holding the author's credentials must not be shipped"
    );
}

#[then(regex = r#"^the declaration is accepted$"#)]
async fn then_declaration_accepted(world: &mut StepWorld) {
    assert!(
        world.bake.refusal.is_none(),
        "unexpected refusal: {:?}",
        world.bake.refusal
    );
}

#[then(regex = r#"^the refusal names "([^"]*)"$"#)]
async fn then_refusal_names(world: &mut StepWorld, needle: String) {
    let message = world
        .bake
        .refusal
        .as_ref()
        .expect("there should be a refusal");
    assert!(
        message.contains(&needle),
        "expected `{needle}` in: {message}"
    );
}
