pub mod camera;
pub mod config;
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
use crate::handlers::{
    camera_capabilities, capture_image, health, openapi_json, recover_camera, swagger_ui,
};
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let request_timeout = std::time::Duration::from_secs(state.config.request_timeout_secs);

    Router::new()
        .route("/api/v1/captures", post(capture_image))
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
    let state = AppState { backend, config };
    build_router(state)
}
