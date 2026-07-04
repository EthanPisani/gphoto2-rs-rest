use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::RwLock;

#[derive(Clone, Default)]
pub struct CaptureControlStore {
    inner: Arc<RwLock<HashMap<String, Arc<AtomicBool>>>>,
}

impl CaptureControlStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register(&self, capture_id: &str) -> Arc<AtomicBool> {
        let token = Arc::new(AtomicBool::new(false));
        let mut map = self.inner.write().await;
        map.insert(capture_id.to_string(), token.clone());
        token
    }

    pub async fn cancel(&self, capture_id: &str) -> bool {
        let map = self.inner.read().await;
        if let Some(token) = map.get(capture_id) {
            token.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub async fn remove(&self, capture_id: &str) {
        let mut map = self.inner.write().await;
        map.remove(capture_id);
    }
}
