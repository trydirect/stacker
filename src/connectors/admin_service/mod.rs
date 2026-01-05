//! Admin Service connector module
//!
//! Provides helper utilities for authenticating internal admin services via JWT tokens.

pub mod jwt;

pub use jwt::{
    JwtClaims,
    parse_jwt_claims,
    validate_jwt_expiration,
    user_from_jwt_claims,
    extract_bearer_token,
};