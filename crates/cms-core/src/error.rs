use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type ApiResult<T> = Result<T, ApiError>;

/// Strapi v5-compatible error envelope (`{ data: null, error: { status, name, message, details } }`).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ErrorEnvelope {
    pub data: Option<serde_json::Value>,
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ErrorBody {
    pub status: u16,
    pub name: String,
    pub message: String,
    pub details: serde_json::Value,
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),

    #[error("{message}")]
    Validation {
        message: String,
        details: serde_json::Value,
    },

    #[error("{0}")]
    Unauthorized(String),

    #[error("{0}")]
    Forbidden(String),

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{0}")]
    RateLimited(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl ApiError {
    pub fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest(_) | Self::Validation { .. } => 400,
            Self::Unauthorized(_) => 401,
            Self::Forbidden(_) => 403,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::RateLimited(_) => 429,
            Self::Internal(_) => 500,
        }
    }

    pub fn error_name(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "BadRequestError",
            Self::Validation { .. } => "ValidationError",
            Self::Unauthorized(_) => "UnauthorizedError",
            Self::Forbidden(_) => "ForbiddenError",
            Self::NotFound(_) => "NotFoundError",
            Self::Conflict(_) => "ConflictError",
            Self::RateLimited(_) => "RateLimitError",
            Self::Internal(_) => "ApplicationError",
        }
    }

    pub fn to_envelope(&self) -> ErrorEnvelope {
        let details = match self {
            Self::Validation { details, .. } => details.clone(),
            _ => serde_json::json!({}),
        };

        let message = match self {
            Self::Internal(_) => "An internal server error occurred".to_string(),
            other => other.to_string(),
        };

        ErrorEnvelope {
            data: None,
            error: ErrorBody {
                status: self.status_code(),
                name: self.error_name().to_string(),
                message,
                details,
            },
        }
    }
}
