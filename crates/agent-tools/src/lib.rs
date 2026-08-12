//! `agent-tools` — pure-logic core for TryDirect's agent-facing MCP tools.
//!
//! Two tools give an AI agent capabilities it structurally lacks:
//! - [`image`] `resolve_image` — ground truth about a Docker image reference.
//! - [`sandbox`] `deploy_ephemeral` — orchestrate a throwaway, auto-expiring
//!   deployment and report its live URL / logs / health.
//!
//! The heavy I/O (Docker Hub, RabbitMQ, DB, cloud provisioning) is injected via
//! the [`image::ImageResolver`] and [`sandbox::SandboxController`] traits, so the
//! decision/assembly logic here compiles and unit-tests apart from the server
//! (`cargo test -p agent-tools`) with mock implementations.

pub mod error;
pub mod image;
pub mod sandbox;
