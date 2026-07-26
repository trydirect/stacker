//! Docker Compose auditing.
//!
//! `validator` is the security scanner shared with the deploy path (moved here
//! as the single source of truth; re-exported so `td_audit::compose::*` keeps
//! exposing `validate_stack_security` etc.). `audit` wraps it (plus compose
//! parsing) into the public [`crate::schema::AuditReport`] shape.

mod validator;
pub use validator::*;

mod audit;
pub use audit::audit_compose;
