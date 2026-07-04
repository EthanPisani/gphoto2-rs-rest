pub mod camera;
pub mod config;
pub mod capture_control;
pub mod capture_store;
pub mod error;
pub mod handlers;
pub mod models;
pub mod openapi;
pub mod state;

use std::sync::Arc;
use std::time::Duration;

use axum::routing::{get, post};
use axum::Router;
use tokio::time::interval;
use tower_http::cors::CorsLayer;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use tracing::{info, warn};

use crate::camera::{CameraBackend, GphotoBackend};
use crate::capture_control::CaptureControlStore;
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

    let capture_store = CaptureStore::new(config.capture_db_path.clone())
        .expect("failed to initialize capture database");

    let state = AppState {
        backend,
        capture_store,
        capture_controls: CaptureControlStore::new(),
        config: config.clone(),
    };

    start_background_tasks(state.clone());

    build_router(state)
}

fn start_background_tasks(state: AppState) {
    let startup_state = state.clone();
    tokio::spawn(async move {
        startup_state
            .capture_store
            .mark_inflight_as_failed("capture interrupted by service restart")
            .await;
    });

    let retention_state = state.clone();
    tokio::spawn(async move {
        let every = Duration::from_secs(retention_state.config.retention_sweep_interval_secs.max(30));
        let mut ticker = interval(every);
        loop {
            ticker.tick().await;
            let deleted = retention_state
                .capture_store
                .sweep_downloaded_older_than(retention_state.config.downloaded_retention_secs)
                .await;
            if deleted > 0 {
                info!(deleted, "retention sweep removed downloaded captures");
            }
        }
    });

    if state.config.keepalive_interval_secs > 0 {
        let keepalive_state = state;
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(keepalive_state.config.keepalive_interval_secs));
            loop {
                ticker.tick().await;
                if let Err(error) = keepalive_state.backend.camera_model().await {
                    warn!(error = %error, "camera keepalive probe failed");
                }
            }
        });
    }
}
