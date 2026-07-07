# Security Audit — Shell Hook Execution

Post-audit ledger for the deploy-hook subsystem (`pre_build`, `post_deploy`, `on_failure`). Documents the concerns raised during the audit, how each was resolved, and where the regression tests live.

For the user-facing execution contract (path rules, trust model, CLI flags, error behaviour) see [`docs/STACKER_YML_REFERENCE.md` § `hooks`](STACKER_YML_REFERENCE.md#hooks).

## Scope

The audit covered the code path added when hook support landed on the `dev` branch:

- `src/console/commands/cli/deploy.rs` — `run_hook`, the deploy pipeline `match` block, `DeployCommand` CLI plumbing, `sanitize_hook_output`.
- `src/cli/install_runner.rs` — `CommandExecutor` trait, `ShellExecutor::execute_with_timeout`, `drain_capped`, `HookPolicy`, `HOOK_PIPE_OUTPUT_MAX_BYTES`.
- `src/cli/config_parser.rs` — `StackerConfig::origin`, `ConfigOrigin`, `MARKETPLACE_ORIGIN_MARKER`, `detect_origin_from_raw`.
- `src/console/commands/cli/marketplace.rs` — `prepend_marketplace_marker`, install write sites.
- `src/helpers/security_validator.rs` — `validate_shell_scripts`, `SHELL_MALICIOUS_PATTERNS`, credential/miner scanners.
- `src/cli/error.rs` — `CliError::HookRejected` variant.
- `src/bin/stacker.rs` — `--no-hooks` / `--allow-untrusted-hooks` CLI flags.

## Status ledger

Severity classes follow the audit report: **C** = critical, **H** = high, **M** = medium, **L** = low, **Docs** = documentation.

| ID  | Concern                                                   | Status                                                                                   |
|-----|-----------------------------------------------------------|------------------------------------------------------------------------------------------|
| C1  | `validate_shell_scripts` dead code                        | Wired into `run_hook` for all three hook slots                                            |
| C2  | Timeout doesn't kill                                      | `Child::spawn` + `try_wait` deadline + `kill` + `wait`                                    |
| C3  | Path traversal / absolute / symlink                       | `canonicalize` + `starts_with(project_dir)` + reject absolute                             |
| C4  | Marketplace-controlled `stacker.yml` hooks                | `# @stacker-origin: marketplace` marker + `HookPolicy` + two CLI flags                    |
| H1 → Phase 6b | Rejection vs runtime taxonomy                | `CliError::HookRejected` split; deploy pipeline classifies                                |
| H2  | Env var leakage into hook                                 | `env_clear` + `PATH` / `HOME` allowlist                                                   |
| H3  | Wrong CWD                                                 | `current_dir(project_dir)`                                                                |
| M1  | Broken regexes                                            | `bash <(curl…)` fixed; +setuid, `rm -rf $HOME/~`, py/perl revshells, `authorized_keys`   |
| M2  | UTF-8 panic in `content.to_lowercase()` sites             | Both fixed via `RegexBuilder::case_insensitive`                                           |
| M3  | Base64 warning noise                                      | Threshold 200 → 1024 at scan site; 100 → 1024 at malicious-code site                      |
| M4  | Unbounded output capture                                  | Phase 8b pipe-level cap + Phase 8 display-level cap                                       |
| M5  | Raw ANSI printed                                          | `strip_ansi_sequences` at print + error-reason boundaries                                 |
| L1  | Test-artefact files at repo root                          | Already in `.gitignore`                                                                   |
| L2 / L3 | Sizing / encoding                                     | Handled by pipe drain + sanitizer                                                         |
| L4  | `--no-hooks` / `--allow-untrusted-hooks`                  | Shipped with C4                                                                           |
| Docs | Hook execution contract                                  | [`STACKER_YML_REFERENCE.md` § `hooks`](STACKER_YML_REFERENCE.md#hooks) — 128 lines         |

## Notes on individual findings

### C1 — validator was dead code

The audit found the fresh `validate_shell_scripts` function was compiled but only ever called by its own tests. Fixed by inserting a call in `run_hook` before executor invocation. Any `[CRITICAL]` finding becomes `CliError::HookRejected`; `[WARNING]` findings are printed to stderr but do not block.

### C2 — timeout was cosmetic

The original `execute_with_timeout` used `std::thread::scope` around a blocking `Command::output()`. The scope's implicit join meant the outer function only returned once the child exited — so the "300-second timeout" never actually terminated anything. Replaced with `Command::spawn` returning a `Child` handle, a `try_wait` deadline loop, and explicit `child.kill()` + `child.wait()` on timeout. Verified by the ignored real-`sleep` test `test_execute_with_timeout_actually_terminates_child`.

### C3 — path rules

- Absolute paths are rejected outright.
- Relative paths are `canonicalize()`d.
- The result must `starts_with()` the canonicalized project directory.
- `symlink_metadata()` used in place of `exists()` so symlinks pointing outside the project are detected.

Rejection short-circuits before the executor is invoked.

### C4 — marketplace trust

`stacker install <template>` prepends `# @stacker-origin: marketplace` to any `stacker.yml` it writes. `StackerConfig::from_file` scans the leading comment block for that marker and sets `origin: ConfigOrigin::MarketplaceGenerated`. `run_hook` refuses to execute against a marketplace-origin config unless the operator passes `--allow-untrusted-hooks` (or removes the marker line after review). `--no-hooks` skips execution regardless of trust.

### Phase 6b — H1 error taxonomy split

Two orthogonal failure modes for hooks:

- **Rejection** (pre-execution): path/policy/security scan refused the hook.
- **Runtime failure** (post-execution): script exited non-zero, or timed out.

The deploy pipeline treats these differently:

| Hook        | Rejection                                                            | Runtime failure                                    |
|-------------|----------------------------------------------------------------------|----------------------------------------------------|
| `pre_build` | Deploy fails.                                                        | Deploy fails.                                      |
| `post_deploy` | Deploy fails.                                                      | Warning logged; deploy stays `Ok`.                 |
| `on_failure` | Original error preserved, rejection chained into the message.       | Warning logged; original error preserved.          |

This is the asymmetry that keeps `post_deploy` / `on_failure` best-effort for legitimate notification / cleanup use cases while ensuring a malicious hook can't be swallowed by a successful deploy.

### H2 — environment scrubbing

`ShellExecutor::execute_with_timeout` calls `Command::env_clear()` before adding back `PATH=/usr/bin:/bin:/usr/local/bin` and `HOME=<inherited>`. Cloud tokens, Docker registry credentials, `GH_TOKEN`, `OPENAI_API_KEY`, and every other secret that lives in the deploying user's shell environment are invisible to hook scripts.

### H3 — working directory

Hooks now execute with `current_dir(project_dir)` set on the `Command`. Previously they inherited the CLI's own CWD.

### M1 — regex fixes and gaps

- `bash <(curl|wget)` regex was checking for a literal `<` followed by a capture group — misparsed. Now `bash\s+<\(?(curl|wget)`.
- Added: `chmod u+s`, `chmod g+s`, `rm -rf $HOME`, `rm -rf ~/`, Python one-liner reverse shells (`socket…connect…dup2…pty.spawn`), Perl one-liner reverse shells (`use Socket`), any reference to `authorized_keys`.

### M2 — UTF-8 panic sites

Both places that used `content.to_lowercase()` + `find()` / `contains()` (crypto miner scanner in `check_no_malicious_code` and `validate_shell_scripts`, plus the default-credential check in `check_no_hardcoded_creds`) allocated a fresh lowercase String and then indexed the *original* content with byte offsets from the lowercase copy. For inputs where lowercasing shrinks the byte length (e.g. `ẞ` → `ß`), the returned offset landed mid-UTF-8-sequence in the original and panicked. All three sites now use `RegexBuilder::new(&regex::escape(pattern)).case_insensitive(true)` on the original content — no lowercased shadow exists.

### M3 — base64 warning threshold

`validate_shell_scripts` had a `{200,}` threshold that fired on every PEM cert body, JWT, or dockerconfigjson blob. Bumped to `{1024,}`. `check_no_malicious_code` had the sibling `{100,}` threshold; also bumped to `{1024,}` for consistency.

### M4 / M5 — output caps and ANSI

Two-layer defence:

1. **Pipe layer** (`ShellExecutor::execute_with_timeout` → `drain_capped`) — each pipe drained in its own thread with a hard byte cap (`HOOK_PIPE_OUTPUT_MAX_BYTES = 1_049_600`). Bytes past the cap are read and discarded so the child never blocks on a full pipe; nothing beyond the cap is ever held in the CLI's memory.
2. **Display layer** (`sanitize_hook_output` in `deploy.rs`) — strips ANSI CSI, OSC, single-char ESC sequences, and stray BEL, then truncates at `HOOK_OUTPUT_MAX_BYTES = 1_048_576` with the marker `\n[stacker: hook output truncated to 1 MiB]`.

Both are invoked unconditionally, so a hook can neither OOM the CLI nor hijack the terminal via escape sequences.

## Test coverage

Regression tests are colocated with the code they protect. Locations:

| Area                              | File                                                        |
|-----------------------------------|-------------------------------------------------------------|
| Validator regex + panic sites     | `src/helpers/security_validator.rs` (tests module)          |
| Path / content / policy rejection | `src/console/commands/cli/deploy.rs` (tests module)         |
| Trust marker + `HookPolicy`       | `src/console/commands/cli/deploy.rs` (`test_c4_*` tests)    |
| Phase 6b error taxonomy           | `src/console/commands/cli/deploy.rs` (post_deploy / on_failure) |
| Output sanitizer                  | `src/console/commands/cli/deploy.rs` (`test_sanitize_*`)    |
| Real timeout + pipe cap           | `src/cli/install_runner.rs` (marked `#[ignore]`)            |

The three pipe-cap tests and the two timeout tests spawn real subprocesses and are gated with `#[ignore]`. Run them explicitly with:

```bash
cargo test -p stacker --lib -- --ignored
```

Everything else runs on the default suite.

## Non-goals

Explicitly out of scope for this audit — noted here so future contributors don't assume they are addressed:

- **Remote hook execution.** Only local hooks (invoked by `stacker deploy`) are covered. The server-side `post_deploy_hooks` JSONB payload and Install Service execution paths are separate concerns.
- **Sandbox / seccomp.** Hooks execute in the same UID/GID as the CLI. Environment scrubbing and CWD pinning are the only isolation applied. Deploys that need stronger isolation should run the CLI itself inside a container.
- **Runtime output tee to file.** Captured output is capped and displayed; no on-disk transcript is kept. If forensics need this, the operator should wrap the hook in `tee`.

## Related documents

- [`STACKER_YML_REFERENCE.md` § `hooks`](STACKER_YML_REFERENCE.md#hooks) — user-facing execution contract.
- [`AI_DEPLOYMENT_WORKFLOWS.md`](AI_DEPLOYMENT_WORKFLOWS.md) — how hooks compose with the AI deploy scenario.
