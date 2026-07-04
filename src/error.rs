use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use thiserror::Error;
use uuid::Uuid;

use crate::models::ErrorResponse;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    Validation(String),
    #[error("resource not found: {0}")]
    NotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("camera not found or unavailable")]
    CameraUnavailable,
    #[error("usb communication failure: {0}")]
    Usb(String),
    #[error("capture failed: {0}")]
    CaptureFailed(String),
    #[error("insufficient storage: {0}")]
    InsufficientStorage(String),
    #[error("internal server error")]
    Internal,
}

impl ApiError {
    fn code(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::CameraUnavailable => "CAMERA_UNAVAILABLE",
            Self::Usb(_) => "USB_CONNECTION_LOST",
            Self::CaptureFailed(_) => "CAPTURE_FAILED",
            Self::InsufficientStorage(_) => "INSUFFICIENT_STORAGE",
            Self::Internal => "INTERNAL_ERROR",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::CameraUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::Usb(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::CaptureFailed(_) => StatusCode::BAD_GATEWAY,
            Self::InsufficientStorage(_) => StatusCode::INSUFFICIENT_STORAGE,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorResponse {
            code: self.code().to_string(),
            message: self.to_string(),
            request_id: Uuid::new_v4().to_string(),
        };
        (status, Json(body)).into_response()
    }
}
