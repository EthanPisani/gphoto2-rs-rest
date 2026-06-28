use utoipa::OpenApi;

use crate::models::{
    CameraCapabilitiesResponse, CaptureRequest, CaptureResponse, ErrorResponse, HealthResponse,
    RecoverResponse,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::capture_image,
        crate::handlers::health,
        crate::handlers::recover_camera,
        crate::handlers::camera_capabilities
    ),
    components(
        schemas(
            CaptureRequest,
            CaptureResponse,
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
