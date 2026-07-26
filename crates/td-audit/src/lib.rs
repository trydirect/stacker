//! `td-audit` — pure-logic audit engines for TryDirect's public "checker" tools.
//!
//! This crate is intentionally free of any web framework or database dependency
//! so it compiles and tests apart from the Actix/sqlx server (`cargo test -p
//! td-audit`). All network I/O (image registries, pricing sources) is injected
//! via traits; the engines themselves are deterministic and unit-testable.
//!
//! Each checker produces a [`schema::AuditReport`] with a shared shape so the
//! frontend renders every tool with one component.

pub mod schema;
pub mod score;

pub mod compose;
pub mod cost;
pub mod dockerfile;
pub mod exposure;
pub mod image;
pub mod readiness;
