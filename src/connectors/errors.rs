use actix_web::{error::ResponseError, http::StatusCode, HttpResponse};
use serde_json::json;
use std::fmt;

/// Errors that can occur during external service communication
#[derive(Debug)]
pub enum ConnectorError {
    /// HTTP request/response error
    HttpError(String),
    /// Service unreachable or timeout
    ServiceUnavailable(String),
    /// Invalid response format from external service
    InvalidResponse(String),
    /// Authentication error (401/403)
    Unauthorized(String),
    /// Not found (404)
    NotFound(String),
    /// Rate limited or exceeded quota
    RateLimited(String),
    /// Internal error in connector
    Internal(String),
    /// Payment required — the external service declined a billing operation
    /// (e.g. no payment method, card declined). Surfaced as HTTP 402 upstream.
    PaymentRequired(String),
    /// Idempotency-key collision or state-transition conflict (HTTP 409).
    Conflict(String),
}

impl fmt::Display for ConnectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HttpError(msg) => write!(f, "HTTP error: {}", msg),
            Self::ServiceUnavailable(msg) => write!(f, "Service unavailable: {}", msg),
            Self::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            Self::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            Self::NotFound(msg) => write!(f, "Not found: {}", msg),
            Self::RateLimited(msg) => write!(f, "Rate limited: {}", msg),
            Self::Internal(msg) => write!(f, "Internal error: {}", msg),
            Self::PaymentRequired(msg) => write!(f, "Payment required: {}", msg),
            Self::Conflict(msg) => write!(f, "Conflict: {}", msg),
        }
    }
}

impl ResponseError for ConnectorError {
    fn error_response(&self) -> HttpResponse {
        let (status, message) = match self {
            Self::HttpError(_) => (StatusCode::BAD_GATEWAY, "External service error"),
            Self::ServiceUnavailable(_) => (StatusCode::SERVICE_UNAVAILABLE, "Service unavailable"),
            Self::InvalidResponse(_) => {
                (StatusCode::BAD_GATEWAY, "Invalid external service response")
            }
            Self::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "Resource not found"),
            Self::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded"),
            Self::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error"),
            Self::PaymentRequired(_) => (StatusCode::PAYMENT_REQUIRED, "Payment required"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "Conflict"),
        };

        HttpResponse::build(status).json(json!({
            "error": message,
            "details": self.to_string(),
        }))
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::HttpError(_) => StatusCode::BAD_GATEWAY,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::InvalidResponse(_) => StatusCode::BAD_GATEWAY,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::PaymentRequired(_) => StatusCode::PAYMENT_REQUIRED,
            Self::Conflict(_) => StatusCode::CONFLICT,
        }
    }
}

impl From<reqwest::Error> for ConnectorError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::ServiceUnavailable(format!("Request timeout: {}", err))
        } else if err.is_connect() {
            Self::ServiceUnavailable(format!("Connection failed: {}", err))
        } else {
            Self::HttpError(err.to_string())
        }
    }
}
