use utoipa::OpenApi;

use crate::models::{
    CameraCapabilitiesResponse, CaptureAccepted, CaptureListResponse, CaptureRecord,
    CaptureRequest, CaptureStatus, ErrorResponse, HealthResponse, RecoverResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::create_capture,
        crate::handlers::get_capture,
        crate::handlers::list_captures,
        crate::handlers::get_capture_file,
        crate::handlers::mark_downloaded,
        crate::handlers::cancel_capture,
        crate::handlers::delete_capture,
        crate::handlers::health,
        crate::handlers::recover_camera,
        crate::handlers::camera_capabilities
    ),
    components(
        schemas(
            CaptureRequest,
            CaptureAccepted,
            CaptureRecord,
            CaptureListResponse,
            CaptureStatus,
            HealthResponse,
            RecoverResponse,
            CameraCapabilitiesResponse,
            ErrorResponse
        )
    ),
    tags(
        (name = "capture", description = "Capture and camera control operations"),
        (name = "health", description = "Service health and recovery operations")
    )
)]
pub struct ApiDoc;
