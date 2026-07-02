use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CaptureRequest {
    #[schema(example = 800)]
    pub iso: Option<String>,
    #[schema(example = "bulb")]
    pub shutter_speed: Option<String>,
    #[schema(example = 30)]
    pub exposure_seconds: Option<u64>,
    #[schema(example = "5.6")]
    pub aperture: Option<String>,
    #[schema(example = "AdobeRGB")]
    pub color_space: Option<String>,
    #[schema(example = "RAW")]
    pub image_format: Option<String>,
    #[schema(example = "Manual")]
    pub exposure_program: Option<String>,
    #[schema(example = "sdram")]
    pub capture_target: Option<String>,
    #[schema(example = true)]
    pub auto_recover_usb: Option<bool>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaptureResponse {
    pub capture_id: String,
    pub camera_model: String,
    pub saved_path: String,
    pub source_folder: String,
    pub source_name: String,
    pub captured_at: DateTime<Utc>,
    pub attempt_count: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Queued,
    Capturing,
    Downloading,
    Complete,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaptureAccepted {
    pub capture_id: String,
    pub status: CaptureStatus,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaptureRecord {
    pub id: String,
    pub status: CaptureStatus,
    pub request_json: String,
    pub camera_model: Option<String>,
    pub saved_path: Option<String>,
    pub source_folder: Option<String>,
    pub source_name: Option<String>,
    pub size_bytes: Option<u64>,
    pub checksum: Option<String>,
    pub error: Option<String>,
    pub attempt_count: u8,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub downloaded_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CaptureListResponse {
    pub items: Vec<CaptureRecord>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ListCapturesQuery {
    pub status: Option<CaptureStatus>,
    pub limit: Option<usize>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct HealthResponse {
    #[schema(example = "ok")]
    pub status: &'static str,
    #[schema(example = "Nikon D5300")]
    pub camera_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RecoverResponse {
    pub status: &'static str,
    pub camera_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CameraCapabilitiesResponse {
    pub camera_model: Option<String>,
    pub supported_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ErrorResponse {
    #[schema(example = "USB_CONNECTION_LOST")]
    pub code: String,
    #[schema(example = "camera disconnected during capture")]
    pub message: String,
    #[schema(example = "4dd6f0f1-35c3-492d-8b31-b7f682ce8c29")]
    pub request_id: String,
}
