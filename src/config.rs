use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub capture_dir: PathBuf,
    pub capture_event_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub camera_retries: u8,
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

        Self {
            bind_addr,
            capture_dir,
            capture_event_timeout_secs,
            request_timeout_secs,
            camera_retries,
        }
    }
}
