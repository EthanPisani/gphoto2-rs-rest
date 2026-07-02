use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::RwLock;

use crate::models::{CaptureRecord, CaptureRequest, CaptureResponse, CaptureStatus};

#[derive(Clone, Default)]
pub struct CaptureStore {
    inner: Arc<RwLock<HashMap<String, CaptureRecord>>>,
}

impl CaptureStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn insert_queued(&self, id: String, request: &CaptureRequest) -> CaptureRecord {
        let request_json = serde_json::to_string(request).unwrap_or_else(|_| "{}".to_string());
        let record = CaptureRecord {
            id: id.clone(),
            status: CaptureStatus::Queued,
            request_json,
            camera_model: None,
            saved_path: None,
            source_folder: None,
            source_name: None,
            size_bytes: None,
            checksum: None,
            error: None,
            attempt_count: 0,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            downloaded_at: None,
        };

        let mut map = self.inner.write().await;
        map.insert(id, record.clone());
        record
    }

    pub async fn insert_queued_if_idle(
        &self,
        id: String,
        request: &CaptureRequest,
    ) -> Option<CaptureRecord> {
        let request_json = serde_json::to_string(request).unwrap_or_else(|_| "{}".to_string());

        let mut map = self.inner.write().await;
        let busy = map.values().any(|record| {
            matches!(
                record.status,
                CaptureStatus::Queued | CaptureStatus::Capturing | CaptureStatus::Downloading
            )
        });

        if busy {
            return None;
        }

        let record = CaptureRecord {
            id: id.clone(),
            status: CaptureStatus::Queued,
            request_json,
            camera_model: None,
            saved_path: None,
            source_folder: None,
            source_name: None,
            size_bytes: None,
            checksum: None,
            error: None,
            attempt_count: 0,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            downloaded_at: None,
        };
        map.insert(id, record.clone());

        Some(record)
    }

    pub async fn set_status(&self, id: &str, status: CaptureStatus) {
        let mut map = self.inner.write().await;
        if let Some(record) = map.get_mut(id) {
            record.status = status;
            if matches!(record.status, CaptureStatus::Capturing) {
                record.started_at = Some(Utc::now());
            }
        }
    }

    pub async fn set_complete(&self, id: &str, response: CaptureResponse) {
        let mut map = self.inner.write().await;
        if let Some(record) = map.get_mut(id) {
            record.status = CaptureStatus::Complete;
            record.camera_model = Some(response.camera_model);
            record.saved_path = Some(response.saved_path.clone());
            record.source_folder = Some(response.source_folder);
            record.source_name = Some(response.source_name);
            record.attempt_count = response.attempt_count;
            record.completed_at = Some(Utc::now());
            record.size_bytes = std::fs::metadata(response.saved_path)
                .ok()
                .map(|meta| meta.len());
        }
    }

    pub async fn set_failed(&self, id: &str, message: String) {
        let mut map = self.inner.write().await;
        if let Some(record) = map.get_mut(id) {
            record.status = CaptureStatus::Failed;
            record.error = Some(message);
            record.completed_at = Some(Utc::now());
        }
    }

    pub async fn set_canceled(&self, id: &str) {
        let mut map = self.inner.write().await;
        if let Some(record) = map.get_mut(id) {
            record.status = CaptureStatus::Canceled;
            record.completed_at = Some(Utc::now());
        }
    }

    pub async fn mark_downloaded(&self, id: &str) {
        let mut map = self.inner.write().await;
        if let Some(record) = map.get_mut(id) {
            record.downloaded_at = Some(Utc::now());
        }
    }

    pub async fn get(&self, id: &str) -> Option<CaptureRecord> {
        let map = self.inner.read().await;
        map.get(id).cloned()
    }

    pub async fn delete(&self, id: &str) -> Option<CaptureRecord> {
        let mut map = self.inner.write().await;
        map.remove(id)
    }

    pub async fn active_capture_exists(&self) -> bool {
        let map = self.inner.read().await;
        map.values().any(|record| {
            matches!(
                record.status,
                CaptureStatus::Queued | CaptureStatus::Capturing | CaptureStatus::Downloading
            )
        })
    }

    pub async fn list(
        &self,
        status: Option<CaptureStatus>,
        limit: usize,
        after: Option<&str>,
    ) -> Vec<CaptureRecord> {
        let map = self.inner.read().await;
        let mut items: Vec<CaptureRecord> = map.values().cloned().collect();

        items.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        if let Some(after_id) = after {
            if let Some(position) = items.iter().position(|item| item.id == after_id) {
                items = items.into_iter().skip(position + 1).collect();
            }
        }

        if let Some(filter_status) = status {
            items.retain(|item| item.status == filter_status);
        }

        items.into_iter().take(limit).collect()
    }
}
