use std::sync::Arc;

use crate::camera::CameraBackend;
use crate::config::AppConfig;

#[derive(Clone)]
pub struct AppState {
    pub backend: Arc<dyn CameraBackend>,
    pub config: AppConfig,
}
