use axum::extract::State;
use axum::response::Html;
use axum::Json;
use utoipa::OpenApi;

use crate::error::ApiError;
use crate::models::{
    CameraCapabilitiesResponse, CaptureRequest, CaptureResponse, ErrorResponse, HealthResponse,
    RecoverResponse,
};
use crate::openapi::ApiDoc;
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/api/v1/captures",
    tag = "capture",
    request_body = CaptureRequest,
    responses(
        (status = 200, description = "Image captured", body = CaptureResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 503, description = "Camera unavailable or USB failure", body = ErrorResponse),
        (status = 502, description = "Capture failed", body = ErrorResponse)
    )
)]
pub async fn capture_image(
    State(state): State<AppState>,
    Json(request): Json<CaptureRequest>,
) -> Result<Json<CaptureResponse>, ApiError> {
    if let Some(exposure_seconds) = request.exposure_seconds {
        if exposure_seconds == 0 {
            return Err(ApiError::Validation(
                "exposure_seconds must be greater than zero".to_string(),
            ));
        }
    }

    let response = state.backend.capture(request).await?;
    Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "Service health", body = HealthResponse),
        (status = 503, description = "Camera unavailable", body = ErrorResponse)
    )
)]
pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    let model = state.backend.camera_model().await?;
    Ok(Json(HealthResponse {
        status: "ok",
        camera_model: model,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/recover",
    tag = "health",
    responses(
        (status = 200, description = "Recovery complete", body = RecoverResponse),
        (status = 503, description = "Recovery failed", body = ErrorResponse)
    )
)]
pub async fn recover_camera(
    State(state): State<AppState>,
) -> Result<Json<RecoverResponse>, ApiError> {
    let model = state.backend.recover().await?;
    Ok(Json(RecoverResponse {
        status: "recovered",
        camera_model: model,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/camera/capabilities",
    tag = "capture",
    responses(
        (status = 200, description = "Camera capability hint list", body = CameraCapabilitiesResponse),
        (status = 503, description = "Camera unavailable", body = ErrorResponse)
    )
)]
pub async fn camera_capabilities(
    State(state): State<AppState>,
) -> Result<Json<CameraCapabilitiesResponse>, ApiError> {
    let model = state.backend.camera_model().await?;
    Ok(Json(CameraCapabilitiesResponse {
        camera_model: model,
        supported_options: vec![
            "iso".to_string(),
            "shutter_speed".to_string(),
            "exposure_seconds".to_string(),
            "aperture".to_string(),
            "color_space".to_string(),
            "image_format".to_string(),
            "exposure_program".to_string(),
            "capture_target".to_string(),
        ],
    }))
}

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

pub async fn swagger_ui() -> Html<&'static str> {
    Html(
        "<!doctype html>
<html>
    <head>
        <meta charset=\"utf-8\" />
        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
        <title>Nikon gphoto2 API Docs</title>
        <link rel=\"stylesheet\" href=\"https://unpkg.com/swagger-ui-dist@5/swagger-ui.css\" />
    </head>
    <body>
        <div id=\"swagger-ui\"></div>
        <script src=\"https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js\"></script>
        <script>
            window.ui = SwaggerUIBundle({
                url: '/api-doc/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [SwaggerUIBundle.presets.apis],
            });
        </script>
    </body>
</html>",
    )
}
