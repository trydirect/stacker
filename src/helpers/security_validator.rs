//! Security validation for stack definitions and shell hooks.
//!
//! The implementation now lives in the `td-audit` crate
//! (`td_audit::compose`) as the single source of truth, shared with the public
//! Compose Auditor checker. This module re-exports it so existing call sites can
//! keep using `crate::helpers::security_validator::*` unchanged.
pub use td_audit::compose::*;
