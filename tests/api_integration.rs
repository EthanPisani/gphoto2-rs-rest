use std::sync::Arc;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use nikon_bulb_server::build_router;
use nikon_bulb_server::camera::CameraBackend;
use nikon_bulb_server::config::AppConfig;
use nikon_bulb_server::error::ApiError;
use nikon_bulb_server::models::{CaptureRequest, CaptureResponse};
use nikon_bulb_server::state::AppState;
use tower::util::ServiceExt;

#[derive(Clone)]
struct MockBackend {
    mode: MockMode,
}

#[derive(Clone)]
enum MockMode {
    Ok,
    Usb,
}

#[async_trait]
impl CameraBackend for MockBackend {
    async fn capture(&self, _request: CaptureRequest) -> Result<CaptureResponse, ApiError> {
        match self.mode {
            MockMode::Ok => Ok(CaptureResponse {
                capture_id: "capture-1".to_string(),
                camera_model: "MockCam".to_string(),
                saved_path: "captures/capture-1.jpg".to_string(),
                source_folder: "/store_00020001/DCIM/100D5300".to_string(),
                source_name: "DSC_0001.JPG".to_string(),
                captured_at: chrono::Utc::now(),
                attempt_count: 1,
            }),
            MockMode::Usb => Err(ApiError::Usb("connection dropped".to_string())),
        }
    }

    async fn recover(&self) -> Result<Option<String>, ApiError> {
        Ok(Some("MockCam".to_string()))
    }

    async fn camera_model(&self) -> Result<Option<String>, ApiError> {
        Ok(Some("MockCam".to_string()))
    }
}

fn app_with_mode(mode: MockMode) -> axum::Router {
    let backend: Arc<dyn CameraBackend> = Arc::new(MockBackend { mode });
    let config = AppConfig::from_env();
    let state = AppState { backend, config };
    build_router(state)
}

#[tokio::test]
async fn capture_success_returns_200() {
    let app = app_with_mode(MockMode::Ok);
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/captures")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"iso":"800","shutter_speed":"bulb","exposure_seconds":5}"#,
        ))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["camera_model"], "MockCam");
}

#[tokio::test]
async fn capture_validation_returns_400() {
    let app = app_with_mode(MockMode::Ok);
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/captures")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"iso":"800","shutter_speed":"bulb","exposure_seconds":0}"#,
        ))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn capture_usb_error_maps_to_503() {
    let app = app_with_mode(MockMode::Usb);
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/captures")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"iso":"800","shutter_speed":"bulb","exposure_seconds":5}"#,
        ))
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn health_returns_200() {
    let app = app_with_mode(MockMode::Ok);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn recover_returns_200() {
    let app = app_with_mode(MockMode::Ok);
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/recover")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
