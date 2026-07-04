use std::sync::Arc;

use crate::camera::CameraBackend;
use crate::capture_control::CaptureControlStore;
use crate::capture_store::CaptureStore;
use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn CameraBackend>,
    pub capture_store: CaptureStore,
    pub capture_controls: CaptureControlStore,
    pub config: AppConfig,
}
