use std::path::PathBuf;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::Json;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use utoipa::OpenApi;
use uuid::Uuid;

use crate::error::ApiError;
use crate::models::{
    CameraCapabilitiesResponse, CaptureAccepted, CaptureListResponse, CaptureRecord, CaptureRequest,
    CaptureStatus, ErrorResponse, HealthResponse, ListCapturesQuery, RecoverResponse,
};
use crate::openapi::ApiDoc;
use crate::state::AppState;

#[utoipa::path(
    post,
    path = "/api/v1/captures",
    tag = "capture",
    request_body = CaptureRequest,
    responses(
        (status = 202, description = "Capture accepted", body = CaptureAccepted),
        (status = 409, description = "Capture already in progress", body = ErrorResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 503, description = "Camera unavailable or USB failure", body = ErrorResponse),
        (status = 502, description = "Capture failed", body = ErrorResponse)
    )
)]
pub async fn create_capture(
    State(state): State<AppState>,
    Json(request): Json<CaptureRequest>,
) -> Result<(StatusCode, Json<CaptureAccepted>), ApiError> {
    if let Some(exposure_seconds) = request.exposure_seconds {
        if exposure_seconds == 0 {
            return Err(ApiError::Validation(
                "exposure_seconds must be greater than zero".to_string(),
            ));
        }
    }

    let capture_id = Uuid::new_v4().to_string();
    let inserted = state
        .capture_store
        .insert_queued_if_idle(capture_id.clone(), &request)
        .await;
    if inserted.is_none() {
        return Err(ApiError::Conflict(
            "only one capture may run at a time".to_string(),
        ));
    }

    let state_for_task = state.clone();
    let capture_id_for_task = capture_id.clone();
    tokio::spawn(async move {
        state_for_task
            .capture_store
            .set_status(&capture_id_for_task, CaptureStatus::Capturing)
            .await;

        match state_for_task.backend.capture(request).await {
            Ok(response) => {
                state_for_task
                    .capture_store
                    .set_status(&capture_id_for_task, CaptureStatus::Downloading)
                    .await;
                state_for_task
                    .capture_store
                    .set_complete(&capture_id_for_task, response)
                    .await;
            }
            Err(error) => {
                state_for_task
                    .capture_store
                    .set_failed(&capture_id_for_task, error.to_string())
                    .await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(CaptureAccepted {
            capture_id,
            status: CaptureStatus::Queued,
        }),
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/captures/{id}",
    tag = "capture",
    responses(
        (status = 200, description = "Capture status", body = CaptureRecord),
        (status = 404, description = "Capture not found", body = ErrorResponse)
    )
)]
pub async fn get_capture(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CaptureRecord>, ApiError> {
    let record = state
        .capture_store
        .get(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("capture '{id}' not found")))?;
    Ok(Json(record))
}

#[utoipa::path(
    get,
    path = "/api/v1/captures",
    tag = "capture",
    params(
        ("status" = Option<String>, Query, description = "Filter by capture status"),
        ("limit" = Option<usize>, Query, description = "Page size"),
        ("after" = Option<String>, Query, description = "Capture id cursor")
    ),
    responses(
        (status = 200, description = "List captures", body = CaptureListResponse)
    )
)]
pub async fn list_captures(
    State(state): State<AppState>,
    Query(query): Query<ListCapturesQuery>,
) -> Json<CaptureListResponse> {
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let items = state
        .capture_store
        .list(query.status, limit, query.after.as_deref())
        .await;
    Json(CaptureListResponse { items })
}

#[utoipa::path(
    post,
    path = "/api/v1/captures/{id}/downloaded",
    tag = "capture",
    responses(
        (status = 200, description = "Capture marked as downloaded", body = CaptureRecord),
        (status = 404, description = "Capture not found", body = ErrorResponse)
    )
)]
pub async fn mark_downloaded(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CaptureRecord>, ApiError> {
    if state.capture_store.get(&id).await.is_none() {
        return Err(ApiError::NotFound(format!("capture '{id}' not found")));
    }

    state.capture_store.mark_downloaded(&id).await;
    let record = state
        .capture_store
        .get(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("capture '{id}' not found")))?;
    Ok(Json(record))
}

#[utoipa::path(
    post,
    path = "/api/v1/captures/{id}/cancel",
    tag = "capture",
    responses(
        (status = 200, description = "Capture canceled", body = CaptureRecord),
        (status = 404, description = "Capture not found", body = ErrorResponse)
    )
)]
pub async fn cancel_capture(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<CaptureRecord>, ApiError> {
    if state.capture_store.get(&id).await.is_none() {
        return Err(ApiError::NotFound(format!("capture '{id}' not found")));
    }

    state.capture_store.set_canceled(&id).await;
    let record = state
        .capture_store
        .get(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("capture '{id}' not found")))?;
    Ok(Json(record))
}

#[utoipa::path(
    delete,
    path = "/api/v1/captures/{id}",
    tag = "capture",
    responses(
        (status = 204, description = "Capture deleted"),
        (status = 404, description = "Capture not found", body = ErrorResponse)
    )
)]
pub async fn delete_capture(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let record = state
        .capture_store
        .delete(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("capture '{id}' not found")))?;

    if let Some(path) = record.saved_path {
        let _ = tokio::fs::remove_file(path).await;
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/captures/{id}/file",
    tag = "capture",
    responses(
        (status = 200, description = "Capture file stream"),
        (status = 206, description = "Partial content"),
        (status = 404, description = "Capture not found", body = ErrorResponse),
        (status = 409, description = "Capture not complete", body = ErrorResponse)
    )
)]
pub async fn get_capture_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let record = state
        .capture_store
        .get(&id)
        .await
        .ok_or_else(|| ApiError::NotFound(format!("capture '{id}' not found")))?;

    if record.status != CaptureStatus::Complete {
        return Err(ApiError::Conflict(format!(
            "capture not ready: {:?}",
            record.status
        )));
    }

    let path = PathBuf::from(record.saved_path.ok_or_else(|| {
        ApiError::CaptureFailed("capture completed but has no saved path".to_string())
    })?);

    serve_file_with_optional_range(path, &headers).await
}

#[utoipa::path(
    get,
    path = "/api/v1/health",
    tag = "health",
    responses(
        (status = 200, description = "Service health", body = HealthResponse),
        (status = 503, description = "Camera unavailable", body = ErrorResponse)
    )
)]
pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    let model = state.backend.camera_model().await?;
    Ok(Json(HealthResponse {
        status: "ok",
        camera_model: model,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/recover",
    tag = "health",
    responses(
        (status = 200, description = "Recovery complete", body = RecoverResponse),
        (status = 503, description = "Recovery failed", body = ErrorResponse)
    )
)]
pub async fn recover_camera(
    State(state): State<AppState>,
) -> Result<Json<RecoverResponse>, ApiError> {
    let model = state.backend.recover().await?;
    Ok(Json(RecoverResponse {
        status: "recovered",
        camera_model: model,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/camera/capabilities",
    tag = "capture",
    responses(
        (status = 200, description = "Camera capability hint list", body = CameraCapabilitiesResponse),
        (status = 503, description = "Camera unavailable", body = ErrorResponse)
    )
)]
pub async fn camera_capabilities(
    State(state): State<AppState>,
) -> Result<Json<CameraCapabilitiesResponse>, ApiError> {
    let model = state.backend.camera_model().await?;
    Ok(Json(CameraCapabilitiesResponse {
        camera_model: model,
        supported_options: vec![
            "iso".to_string(),
            "shutter_speed".to_string(),
            "exposure_seconds".to_string(),
            "aperture".to_string(),
            "color_space".to_string(),
            "image_format".to_string(),
            "exposure_program".to_string(),
            "capture_target".to_string(),
        ],
    }))
}

fn parse_single_range(range_header: &str, file_size: u64) -> Option<(u64, u64)> {
    let prefix = "bytes=";
    if !range_header.starts_with(prefix) {
        return None;
    }

    let raw = &range_header[prefix.len()..];
    if raw.contains(',') {
        return None;
    }

    let (start_raw, end_raw) = raw.split_once('-')?;

    if start_raw.is_empty() {
        let suffix_len = end_raw.parse::<u64>().ok()?;
        let start = file_size.saturating_sub(suffix_len);
        let end = file_size.saturating_sub(1);
        return Some((start, end));
    }

    let start = start_raw.parse::<u64>().ok()?;
    let end = if end_raw.is_empty() {
        file_size.saturating_sub(1)
    } else {
        end_raw.parse::<u64>().ok()?.min(file_size.saturating_sub(1))
    };

    if start > end || start >= file_size {
        return None;
    }

    Some((start, end))
}

async fn serve_file_with_optional_range(
    path: PathBuf,
    headers: &HeaderMap,
) -> Result<Response, ApiError> {
    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|_| ApiError::NotFound(format!("file '{}' not found", path.display())))?;
    let size = metadata.len();

    let content_type = HeaderValue::from_static("application/octet-stream");
    let path_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "capture.bin".to_string());

    if let Some(range_value) = headers.get(header::RANGE) {
        let range_str = range_value
            .to_str()
            .map_err(|_| ApiError::Validation("invalid range header".to_string()))?;

        let (start, end) = parse_single_range(range_str, size)
            .ok_or_else(|| ApiError::Validation("unsupported or invalid range".to_string()))?;
        let length = end - start + 1;

        let mut file = File::open(&path)
            .await
            .map_err(|_| ApiError::NotFound(format!("file '{}' not found", path.display())))?;
        file.seek(SeekFrom::Start(start))
            .await
            .map_err(|_| ApiError::Internal)?;

        let mut data = vec![0u8; length as usize];
        file.read_exact(&mut data).await.map_err(|_| ApiError::Internal)?;

        let mut response = Response::new(Body::from(data));
        *response.status_mut() = StatusCode::PARTIAL_CONTENT;
        response
            .headers_mut()
            .insert(header::CONTENT_TYPE, content_type.clone());
        response.headers_mut().insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&format!("attachment; filename=\"{path_name}\""))
                .map_err(|_| ApiError::Internal)?,
        );
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            HeaderValue::from_str(&format!("bytes {start}-{end}/{size}"))
                .map_err(|_| ApiError::Internal)?,
        );
        response.headers_mut().insert(
            header::ACCEPT_RANGES,
            HeaderValue::from_static("bytes"),
        );
        return Ok(response);
    }

    let file = File::open(&path)
        .await
        .map_err(|_| ApiError::NotFound(format!("file '{}' not found", path.display())))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut response = body.into_response();
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, content_type);
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{path_name}\""))
            .map_err(|_| ApiError::Internal)?,
    );
    response.headers_mut().insert(
        header::ACCEPT_RANGES,
        HeaderValue::from_static("bytes"),
    );

    Ok(response)
}

pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    Json(ApiDoc::openapi())
}

pub async fn swagger_ui() -> Html<&'static str> {
    Html(
        "<!doctype html>
<html>
    <head>
        <meta charset=\"utf-8\" />
        <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />
        <title>Nikon gphoto2 API Docs</title>
        <link rel=\"stylesheet\" href=\"https://unpkg.com/swagger-ui-dist@5/swagger-ui.css\" />
    </head>
    <body>
        <div id=\"swagger-ui\"></div>
        <script src=\"https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js\"></script>
        <script>
            window.ui = SwaggerUIBundle({
                url: '/api-doc/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [SwaggerUIBundle.presets.apis],
            });
        </script>
    </body>
</html>",
    )
}
