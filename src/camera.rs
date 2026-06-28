use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use gphoto2::widget::Widget;
use gphoto2::{Camera, Context};
use tokio::task;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{CaptureRequest, CaptureResponse};

#[async_trait]
pub trait CameraBackend: Send + Sync {
    async fn capture(&self, request: CaptureRequest) -> Result<CaptureResponse, ApiError>;
    async fn recover(&self) -> Result<Option<String>, ApiError>;
    async fn camera_model(&self) -> Result<Option<String>, ApiError>;
}

#[derive(Clone)]
pub struct GphotoBackend {
    pub capture_dir: PathBuf,
    pub event_timeout: Duration,
    pub retries: u8,
}

impl GphotoBackend {
    pub fn new(capture_dir: PathBuf, event_timeout: Duration, retries: u8) -> Self {
        Self {
            capture_dir,
            event_timeout,
            retries,
        }
    }
}

#[async_trait]
impl CameraBackend for GphotoBackend {
    async fn capture(&self, request: CaptureRequest) -> Result<CaptureResponse, ApiError> {
        let max_attempts = self.retries.saturating_add(1);
        let mut last_error: Option<ApiError> = None;

        for attempt in 1..=max_attempts {
            let capture_dir = self.capture_dir.clone();
            let event_timeout = self.event_timeout;
            let request_clone = request.clone();

            let result = task::spawn_blocking(move || {
                capture_once(capture_dir, event_timeout, request_clone, attempt)
            })
            .await
            .map_err(|_| ApiError::Internal)?;

            match result {
                Ok(response) => return Ok(response),
                Err(err @ ApiError::Usb(_)) | Err(err @ ApiError::CameraUnavailable) => {
                    last_error = Some(err);
                    continue;
                }
                Err(err) => return Err(err),
            }
        }

        Err(last_error.unwrap_or(ApiError::CameraUnavailable))
    }

    async fn recover(&self) -> Result<Option<String>, ApiError> {
        self.camera_model().await
    }

    async fn camera_model(&self) -> Result<Option<String>, ApiError> {
        let result = task::spawn_blocking(move || {
            let context = Context::new().map_err(|e| ApiError::Usb(e.to_string()))?;
            let camera = context
                .autodetect_camera()
                .wait()
                .map_err(|_| ApiError::CameraUnavailable)?;
            let model = camera.abilities().model().to_string();
            Ok::<Option<String>, ApiError>(Some(model))
        })
        .await
        .map_err(|_| ApiError::Internal)??;

        Ok(result)
    }
}

fn capture_once(
    capture_dir: PathBuf,
    event_timeout: Duration,
    request: CaptureRequest,
    attempt_count: u8,
) -> Result<CaptureResponse, ApiError> {
    std::fs::create_dir_all(&capture_dir)
        .map_err(|e| ApiError::CaptureFailed(format!("create capture dir failed: {e}")))?;

    let context = Context::new().map_err(|e| ApiError::Usb(e.to_string()))?;
    let camera = context
        .autodetect_camera()
        .wait()
        .map_err(|_| ApiError::CameraUnavailable)?;

    let camera_model = camera.abilities().model().to_string();

    if let Some(exposure_program) = request.exposure_program.as_deref() {
        set_camera_option(&camera, "expprogram", exposure_program)?;
    }
    if let Some(iso) = request.iso.as_deref() {
        set_camera_option(&camera, "iso", iso)?;
    }
    if let Some(aperture) = request.aperture.as_deref() {
        set_camera_option(&camera, "f-number", aperture)?;
    }
    if let Some(color_space) = request.color_space.as_deref() {
        set_camera_option(&camera, "colorspace", color_space)?;
    }
    if let Some(image_format) = request.image_format.as_deref() {
        set_camera_option(&camera, "imagequality", image_format)?;
    }
    if let Some(capture_target) = request.capture_target.as_deref() {
        set_capture_target(&camera, capture_target)?;
    }

    let capture_id = Uuid::new_v4().to_string();

    let shutter_speed = request.shutter_speed.as_deref().unwrap_or("bulb");
    let capture_path = if shutter_speed.eq_ignore_ascii_case("bulb") {
        let exposure_seconds = request.exposure_seconds.ok_or_else(|| {
            ApiError::Validation("exposure_seconds is required for bulb mode".to_string())
        })?;

        // Fix 1: remove the silent 30s default — make it required or use a sane large default
        let exposure_seconds = request.exposure_seconds.ok_or_else(|| {
            ApiError::Validation("exposure_seconds is required for bulb mode".to_string())
        })?;

        let extension = extension_from_image_format(request.image_format.as_deref());
        let output_file_name = format!("{capture_id}.{extension}");
        let output_path = capture_dir.join(&output_file_name);

        // Fix 2: use the native libgphoto2 path, not the CLI
        let output_path = capture_bulb_native(
            &camera,
            &capture_dir,
            exposure_seconds,
            &capture_id,
            request.image_format.as_deref(),
        )?;

        return Ok(CaptureResponse {
            capture_id,
            camera_model,
            saved_path: output_path.to_string_lossy().to_string(),
            source_folder: "local".to_string(),
            source_name: output_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            captured_at: Utc::now(),
            attempt_count,
        });
    } else {
        set_camera_option(&camera, "shutterspeed", shutter_speed)?;
        camera
            .capture_image()
            .wait()
            .map_err(|e| ApiError::CaptureFailed(format!("capture_image failed: {e}")))?
    };

    let source_folder = capture_path.folder().to_string();
    let source_name = capture_path.name().to_string();
    let extension = source_name.rsplit('.').next().unwrap_or("jpg");
    let file_name = format!("{capture_id}.{extension}");
    let output_path = capture_dir.join(file_name);

    camera
        .fs()
        .download_to(&source_folder, &source_name, &output_path)
        .wait()
        .map_err(|e| ApiError::CaptureFailed(format!("save failed: {e}")))?;

    Ok(CaptureResponse {
        capture_id,
        camera_model,
        saved_path: output_path.to_string_lossy().to_string(),
        source_folder,
        source_name,
        captured_at: Utc::now(),
        attempt_count,
    })
}

fn set_capture_target(camera: &Camera, target: &str) -> Result<(), ApiError> {
    let normalized = target.to_ascii_lowercase();
    let candidates: &[&str] = match normalized.as_str() {
        "sdram" | "ram" => &["sdram", "Internal RAM", "Memory buffer", "memory buffer"],
        "card" | "sd" | "sdcard" => &["card", "Memory card", "memory card"],
        _ => {
            return Err(ApiError::Validation(
                "capture_target must be one of: sdram, card".to_string(),
            ));
        }
    };

    for candidate in candidates {
        if set_camera_option(camera, "capturetarget", candidate).is_ok() {
            return Ok(());
        }
    }

    Err(ApiError::Validation(format!(
        "unable to set capture_target '{}' on this camera",
        target
    )))
}
fn capture_bulb_native(
    camera: &Camera,
    capture_dir: &std::path::Path,
    exposure_seconds: u64,
    capture_id: &str,
    image_format: Option<&str>,
) -> Result<std::path::PathBuf, ApiError> {
    use gphoto2::camera::CameraEvent;
    use gphoto2::widget::Widget;
    use std::thread;
    use std::time::Duration;

    // Open shutter: set the bulb toggle to 1
    let widget = camera
        .config_key::<Widget>("bulb")
        .wait()
        .map_err(|e| ApiError::CaptureFailed(format!("read bulb config failed: {e}")))?;

    let Widget::Toggle(ref bulb_widget) = widget else {
        return Err(ApiError::CaptureFailed(
            "bulb config is not a toggle widget".to_string(),
        ));
    };

    bulb_widget.set_toggled(true);
    camera
        .set_config(bulb_widget)
        .wait()
        .map_err(|e| ApiError::CaptureFailed(format!("bulb open failed: {e}")))?;
    println!("Bulb open for {exposure_seconds} seconds...");
    // Hold for the requested duration
    thread::sleep(Duration::from_secs(exposure_seconds));
    println!("Bulb close after {exposure_seconds} seconds...");

    // Close shutter: set bulb toggle to 0
    // Note: D5300 may return an error on set_config(false) even when it works —
    // ignore the error and proceed to wait for the capture event regardless
    let widget2 = camera
        .config_key::<Widget>("bulb")
        .wait()
        .map_err(|e| ApiError::CaptureFailed(format!("read bulb config (close) failed: {e}")))?;

    if let Widget::Toggle(ref bulb_close) = widget2 {
        bulb_close.set_toggled(false);
        let _ = camera.set_config(bulb_close).wait(); // intentionally ignore error — D5300 known bug
    }

    // Drain events until we get a NewFile or CaptureComplete (up to 30s grace)
    let deadline = Duration::from_secs(30);
    let tick = Duration::from_millis(500);
    let mut elapsed = Duration::ZERO;
    let mut capture_path: Option<(String, String)> = None;

    while elapsed < deadline {
        match camera.wait_event(tick).wait() {
            Ok(CameraEvent::NewFile(path)) => {
                capture_path = Some((path.folder().to_string(), path.name().to_string()));
                break;
            }
            Ok(CameraEvent::CaptureComplete) => {
                // image is still being written; keep draining for NewFile
            }
            _ => {}
        }
        elapsed += tick;
    }

    let (folder, name) = capture_path.ok_or_else(|| {
        ApiError::CaptureFailed("no NewFile event received after bulb close".to_string())
    })?;

    let extension = name.rsplit('.').next().unwrap_or("nef");
    let file_name = format!("{capture_id}.{extension}");
    let output_path = capture_dir.join(&file_name);

    camera
        .fs()
        .download_to(&folder, &name, &output_path)
        .wait()
        .map_err(|e| ApiError::CaptureFailed(format!("download failed: {e}")))?;

    Ok(output_path)
}
fn capture_bulb_with_gphoto2_cli(
    exposure_seconds: u64,
    output_path: &std::path::Path,
) -> Result<(), ApiError> {
    let status = Command::new("gphoto2")
        .arg("-B")
        .arg(exposure_seconds.to_string())
        .arg("--capture-image-and-download")
        .arg("--force-overwrite")
        .arg("--filename")
        .arg(output_path.as_os_str())
        .status()
        .map_err(|e| ApiError::CaptureFailed(format!("failed to execute gphoto2: {e}")))?;

    if !status.success() {
        return Err(ApiError::CaptureFailed(format!(
            "gphoto2 bulb capture failed with status: {status}"
        )));
    }

    Ok(())
}

fn extension_from_image_format(image_format: Option<&str>) -> &'static str {
    let Some(format) = image_format else {
        return "jpg";
    };

    let normalized = format.to_ascii_lowercase();
    if normalized.contains("raw") || normalized.contains("nef") {
        "nef"
    } else if normalized.contains("jpeg") || normalized.contains("jpg") {
        "jpg"
    } else {
        "jpg"
    }
}

fn set_camera_option(camera: &Camera, key: &str, value: &str) -> Result<(), ApiError> {
    let widget = camera
        .config_key::<Widget>(key)
        .wait()
        .map_err(|e| ApiError::CaptureFailed(format!("read {key} config failed: {e}")))?;

    match widget {
        Widget::Radio(w) => {
            w.set_choice(value)
                .map_err(|e| ApiError::Validation(format!("invalid {key} value '{value}': {e}")))?;
            camera
                .set_config(&w)
                .wait()
                .map_err(|e| ApiError::CaptureFailed(format!("set {key} failed: {e}")))?;
        }
        Widget::Text(w) => {
            w.set_value(value)
                .map_err(|e| ApiError::Validation(format!("invalid {key} value '{value}': {e}")))?;
            camera
                .set_config(&w)
                .wait()
                .map_err(|e| ApiError::CaptureFailed(format!("set {key} failed: {e}")))?;
        }
        Widget::Toggle(w) => {
            let parsed = matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "on");
            w.set_toggled(parsed);
            camera
                .set_config(&w)
                .wait()
                .map_err(|e| ApiError::CaptureFailed(format!("set {key} failed: {e}")))?;
        }
        Widget::Range(w) => {
            let parsed: f32 = value
                .parse()
                .map_err(|_| ApiError::Validation(format!("{key} must be numeric")))?;
            w.set_value(parsed);
            camera
                .set_config(&w)
                .wait()
                .map_err(|e| ApiError::CaptureFailed(format!("set {key} failed: {e}")))?;
        }
        Widget::Date(w) => {
            let parsed: i32 = value
                .parse()
                .map_err(|_| ApiError::Validation(format!("{key} must be a unix timestamp")))?;
            w.set_timestamp(parsed);
            camera
                .set_config(&w)
                .wait()
                .map_err(|e| ApiError::CaptureFailed(format!("set {key} failed: {e}")))?;
        }
        Widget::Button(_) | Widget::Group(_) => {
            return Err(ApiError::Validation(format!(
                "camera option '{key}' cannot be set directly"
            )));
        }
    }

    Ok(())
}
