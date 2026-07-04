use std::path::PathBuf;
use std::time::Duration;

use nikon_bulb_server::camera::{CameraBackend, GphotoBackend};
use nikon_bulb_server::error::ApiError;
use nikon_bulb_server::models::CaptureRequest;

fn hardware_test_enabled() -> bool {
    std::env::var("RUN_HARDWARE_TESTS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

#[tokio::test]
#[ignore = "Requires a connected Nikon D5300 and RUN_HARDWARE_TESTS=1"]
async fn d5300_health_check_and_capture() {
    if !hardware_test_enabled() {
        eprintln!("Skipping because RUN_HARDWARE_TESTS is not enabled");
        return;
    }

    let backend = GphotoBackend::new(PathBuf::from("captures"), Duration::from_secs(20), 1);

    let model = backend
        .camera_model()
        .await
        .expect("camera_model call failed");

    let model_name = model.expect("camera not detected");
    assert!(
        model_name.contains("Nikon") || model_name.contains("D5300"),
        "expected Nikon camera, got: {}",
        model_name
    );

    let response = capture_with_fallbacks(&backend)
        .await
        .expect("capture failed with all fallback requests");

    let saved_path = PathBuf::from(&response.saved_path);
    assert!(
        saved_path.exists(),
        "captured file not found: {}",
        response.saved_path
    );
}

async fn capture_with_fallbacks(
    backend: &GphotoBackend,
) -> Result<nikon_bulb_server::models::CaptureResponse, String> {
    let candidates = vec![
        CaptureRequest {
            iso: Some("200".to_string()),
            shutter_speed: Some("1/60".to_string()),
            exposure_seconds: None,
            aperture: None,
            color_space: None,
            image_format: None,
            exposure_program: None,
            capture_target: Some("sdram".to_string()),
            auto_recover_usb: Some(true),
        },
        CaptureRequest {
            iso: Some("200".to_string()),
            shutter_speed: Some("1/125".to_string()),
            exposure_seconds: None,
            aperture: None,
            color_space: None,
            image_format: None,
            exposure_program: None,
            capture_target: Some("sdram".to_string()),
            auto_recover_usb: Some(true),
        },
        CaptureRequest {
            iso: None,
            shutter_speed: Some("1/60".to_string()),
            exposure_seconds: None,
            aperture: None,
            color_space: None,
            image_format: None,
            exposure_program: None,
            capture_target: Some("sdram".to_string()),
            auto_recover_usb: Some(true),
        },
    ];

    let mut last_error = String::new();
    for req in candidates {
        match backend.capture(req).await {
            Ok(res) => return Ok(res),
            Err(err) => {
                last_error = format_api_error(err);
            }
        }
    }

    Err(last_error)
}

fn format_api_error(err: ApiError) -> String {
    match err {
        ApiError::Validation(m) => format!("validation: {m}"),
        ApiError::NotFound(m) => format!("not found: {m}"),
        ApiError::Conflict(m) => format!("conflict: {m}"),
        ApiError::CameraUnavailable => "camera unavailable".to_string(),
        ApiError::Usb(m) => format!("usb: {m}"),
        ApiError::CaptureFailed(m) => format!("capture failed: {m}"),
        ApiError::InsufficientStorage(m) => format!("insufficient storage: {m}"),
        ApiError::Internal => "internal".to_string(),
    }
}
