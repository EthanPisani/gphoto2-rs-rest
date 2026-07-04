use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use nikon_bulb_server::build_router;
use nikon_bulb_server::camera::CameraBackend;
use nikon_bulb_server::capture_control::CaptureControlStore;
use nikon_bulb_server::capture_store::CaptureStore;
use nikon_bulb_server::config::AppConfig;
use nikon_bulb_server::error::ApiError;
use nikon_bulb_server::models::{CaptureRequest, CaptureResponse};
use nikon_bulb_server::state::AppState;
use tower::util::ServiceExt;
use tokio::time::{sleep, Duration};

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
    async fn capture_with_cancel(
        &self,
        request: CaptureRequest,
        _cancel_token: Arc<AtomicBool>,
    ) -> Result<CaptureResponse, ApiError> {
        self.capture(request).await
    }

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
    let capture_store = CaptureStore::new(std::env::temp_dir().join(format!(
        "nikon-bulb-test-{}.db",
        uuid::Uuid::new_v4()
    )))
    .expect("test capture store init");
    let state = AppState {
        backend,
        capture_store,
        capture_controls: CaptureControlStore::new(),
        config,
    };
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
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "queued");
    assert!(json["capture_id"].as_str().is_some());
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
async fn capture_usb_error_transitions_to_failed() {
    let app = app_with_mode(MockMode::Usb);
    let create_req = Request::builder()
        .method("POST")
        .uri("/api/v1/captures")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"iso":"800","shutter_speed":"bulb","exposure_seconds":5}"#,
        ))
        .unwrap();

    let response = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let accepted: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let capture_id = accepted["capture_id"].as_str().unwrap();

    sleep(Duration::from_millis(20)).await;

    let status_req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/captures/{capture_id}"))
        .body(Body::empty())
        .unwrap();

    let status_response = app.oneshot(status_req).await.unwrap();
    assert_eq!(status_response.status(), StatusCode::OK);
    let status_bytes = status_response.into_body().collect().await.unwrap().to_bytes();
    let status_json: serde_json::Value = serde_json::from_slice(&status_bytes).unwrap();
    assert_eq!(status_json["status"], "failed");
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
