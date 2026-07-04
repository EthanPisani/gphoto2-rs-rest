use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub capture_dir: PathBuf,
    pub capture_db_path: PathBuf,
    pub capture_event_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub camera_retries: u8,
    pub min_free_space_bytes: u64,
    pub downloaded_retention_secs: u64,
    pub retention_sweep_interval_secs: u64,
    pub keepalive_interval_secs: u64,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let bind_addr = std::env::var("BIND_ADDR")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| "0.0.0.0:8080".parse().expect("valid default bind address"));

        let capture_dir = std::env::var("CAPTURE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("captures"));

        let capture_db_path = std::env::var("CAPTURE_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| capture_dir.join("captures.db"));

        let capture_event_timeout_secs = std::env::var("CAPTURE_EVENT_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(20);

        let request_timeout_secs = std::env::var("REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(900);

        let camera_retries = std::env::var("CAMERA_RETRIES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(2);

        let min_free_space_bytes = std::env::var("MIN_FREE_SPACE_BYTES")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(512 * 1024 * 1024);

        let downloaded_retention_secs = std::env::var("DOWNLOADED_RETENTION_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(24 * 60 * 60);

        let retention_sweep_interval_secs = std::env::var("RETENTION_SWEEP_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10 * 60);

        let keepalive_interval_secs = std::env::var("KEEPALIVE_INTERVAL_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(2 * 60);

        Self {
            bind_addr,
            capture_dir,
            capture_db_path,
            capture_event_timeout_secs,
            request_timeout_secs,
            camera_retries,
            min_free_space_bytes,
            downloaded_retention_secs,
            retention_sweep_interval_secs,
            keepalive_interval_secs,
        }
    }
}
