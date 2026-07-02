pub mod camera;
pub mod config;
pub mod capture_store;
pub mod error;
pub mod handlers;
pub mod models;
pub mod openapi;
pub mod state;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::camera::{CameraBackend, GphotoBackend};
use crate::config::AppConfig;
use crate::capture_store::CaptureStore;
use crate::handlers::{
    camera_capabilities, cancel_capture, create_capture, delete_capture, get_capture,
    get_capture_file, health, list_captures, mark_downloaded, openapi_json, recover_camera,
    swagger_ui,
};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let request_timeout = std::time::Duration::from_secs(state.config.request_timeout_secs);

    Router::new()
        .route("/api/v1/captures", post(create_capture).get(list_captures))
        .route("/api/v1/captures/:id", get(get_capture).delete(delete_capture))
        .route("/api/v1/captures/:id/file", get(get_capture_file))
        .route("/api/v1/captures/:id/downloaded", post(mark_downloaded))
        .route("/api/v1/captures/:id/cancel", post(cancel_capture))
        .route("/api/v1/health", get(health))
        .route("/api/v1/recover", post(recover_camera))
        .route("/api/v1/camera/capabilities", get(camera_capabilities))
        .route("/api-doc/openapi.json", get(openapi_json))
        .route("/swagger-ui", get(swagger_ui))
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(TimeoutLayer::new(request_timeout))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub fn build_default_router(config: AppConfig) -> Router {
    let backend: Arc<dyn CameraBackend> = Arc::new(GphotoBackend::new(
        config.capture_dir.clone(),
        std::time::Duration::from_secs(config.capture_event_timeout_secs),
        config.camera_retries,
    ));
    let state = AppState {
        backend,
        capture_store: CaptureStore::new(),
        config,
    };
    build_router(state)
}
