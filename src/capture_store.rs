use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::models::{CaptureRecord, CaptureRequest, CaptureResponse, CaptureStatus};

#[derive(Clone)]
pub struct CaptureStore {
    db_path: PathBuf,
}

impl CaptureStore {
    pub fn new(db_path: PathBuf) -> Result<Self, rusqlite::Error> {
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS captures (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                request_json TEXT NOT NULL,
                camera_model TEXT,
                saved_path TEXT,
                source_folder TEXT,
                source_name TEXT,
                size_bytes INTEGER,
                checksum TEXT,
                error TEXT,
                attempt_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                downloaded_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_captures_status_created
                ON captures(status, created_at);
            CREATE INDEX IF NOT EXISTS idx_captures_downloaded_at
                ON captures(downloaded_at);
            ",
        )?;

        Ok(Self { db_path })
    }

    pub async fn mark_inflight_as_failed(&self, reason: &str) {
        let db_path = self.db_path.clone();
        let reason = reason.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            conn.execute(
                "
                UPDATE captures
                SET status = 'failed',
                    error = ?,
                    completed_at = ?
                WHERE status IN ('queued', 'capturing', 'downloading')
                ",
                params![reason, Utc::now().to_rfc3339()],
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await;
    }

    pub async fn insert_queued_if_idle(
        &self,
        id: String,
        request: &CaptureRequest,
    ) -> Option<CaptureRecord> {
        let db_path = self.db_path.clone();
        let request_json = serde_json::to_string(request).unwrap_or_else(|_| "{}".to_string());
        tokio::task::spawn_blocking(move || {
            let mut conn = Connection::open(db_path).ok()?;
            let tx = conn.transaction().ok()?;

            let active_count: i64 = tx
                .query_row(
                    "SELECT COUNT(1) FROM captures WHERE status IN ('queued', 'capturing', 'downloading')",
                    [],
                    |row| row.get(0),
                )
                .ok()?;

            if active_count > 0 {
                return None;
            }

            let now = Utc::now();
            tx.execute(
                "
                INSERT INTO captures(id, status, request_json, attempt_count, created_at)
                VALUES (?, 'queued', ?, 0, ?)
                ",
                params![id, request_json, now.to_rfc3339()],
            )
            .ok()?;

            tx.commit().ok()?;

            Some(CaptureRecord {
                id,
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
                created_at: now,
                started_at: None,
                completed_at: None,
                downloaded_at: None,
            })
        })
        .await
        .ok()
        .flatten()
    }

    pub async fn set_status(&self, id: &str, status: CaptureStatus) {
        let db_path = self.db_path.clone();
        let id = id.to_string();
        let status_text = status_to_str(status);
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            if status_text == "capturing" {
                conn.execute(
                    "UPDATE captures SET status = ?, started_at = ? WHERE id = ?",
                    params![status_text, Utc::now().to_rfc3339(), id],
                )?;
            } else {
                conn.execute(
                    "UPDATE captures SET status = ? WHERE id = ?",
                    params![status_text, id],
                )?;
            }
            Ok::<(), rusqlite::Error>(())
        })
        .await;
    }

    pub async fn set_complete(&self, id: &str, response: CaptureResponse) {
        let db_path = self.db_path.clone();
        let id = id.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            let size_bytes = std::fs::metadata(&response.saved_path)
                .ok()
                .map(|meta| meta.len() as i64);
            let checksum = sha256_of_file(&response.saved_path);

            conn.execute(
                "
                UPDATE captures
                SET status = 'complete',
                    camera_model = ?,
                    saved_path = ?,
                    source_folder = ?,
                    source_name = ?,
                    size_bytes = ?,
                    checksum = ?,
                    attempt_count = ?,
                    completed_at = ?
                WHERE id = ?
                ",
                params![
                    response.camera_model,
                    response.saved_path,
                    response.source_folder,
                    response.source_name,
                    size_bytes,
                    checksum,
                    response.attempt_count,
                    Utc::now().to_rfc3339(),
                    id
                ],
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await;
    }

    pub async fn set_failed(&self, id: &str, message: String) {
        let db_path = self.db_path.clone();
        let id = id.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            conn.execute(
                "
                UPDATE captures
                SET status = 'failed', error = ?, completed_at = ?
                WHERE id = ?
                ",
                params![message, Utc::now().to_rfc3339(), id],
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await;
    }

    pub async fn set_canceled(&self, id: &str) {
        let db_path = self.db_path.clone();
        let id = id.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            conn.execute(
                "
                UPDATE captures
                SET status = 'canceled', completed_at = ?
                WHERE id = ? AND status IN ('queued', 'capturing', 'downloading')
                ",
                params![Utc::now().to_rfc3339(), id],
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await;
    }

    pub async fn mark_downloaded(&self, id: &str) {
        let db_path = self.db_path.clone();
        let id = id.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            conn.execute(
                "UPDATE captures SET downloaded_at = ? WHERE id = ?",
                params![Utc::now().to_rfc3339(), id],
            )?;
            Ok::<(), rusqlite::Error>(())
        })
        .await;
    }

    pub async fn get(&self, id: &str) -> Option<CaptureRecord> {
        let db_path = self.db_path.clone();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path).ok()?;
            let mut stmt = conn
                .prepare(
                    "
                    SELECT id, status, request_json, camera_model, saved_path, source_folder,
                           source_name, size_bytes, checksum, error, attempt_count,
                           created_at, started_at, completed_at, downloaded_at
                    FROM captures
                    WHERE id = ?
                    ",
                )
                .ok()?;

            let mut rows = stmt.query(params![id]).ok()?;
            let row = rows.next().ok().flatten()?;
            row_to_record(row).ok()
        })
        .await
        .ok()
        .flatten()
    }

    pub async fn delete(&self, id: &str) -> Option<CaptureRecord> {
        let record = self.get(id).await;
        if record.is_none() {
            return None;
        }

        let db_path = self.db_path.clone();
        let id = id.to_string();
        let _ = tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path)?;
            conn.execute("DELETE FROM captures WHERE id = ?", params![id])?;
            Ok::<(), rusqlite::Error>(())
        })
        .await;
        record
    }

    pub async fn list(
        &self,
        status: Option<CaptureStatus>,
        limit: usize,
        after: Option<&str>,
    ) -> Vec<CaptureRecord> {
        let db_path = self.db_path.clone();
        let status = status.map(status_to_str);
        let limit = limit as i64;
        let after = after.map(|s| s.to_string());

        tokio::task::spawn_blocking(move || {
            let conn = match Connection::open(db_path) {
                Ok(c) => c,
                Err(_) => return Vec::new(),
            };

            let query = "
                SELECT id, status, request_json, camera_model, saved_path, source_folder,
                       source_name, size_bytes, checksum, error, attempt_count,
                       created_at, started_at, completed_at, downloaded_at
                FROM captures
                WHERE (?1 IS NULL OR status = ?1)
                  AND (
                    ?2 IS NULL OR
                    created_at > COALESCE((SELECT created_at FROM captures WHERE id = ?2), '')
                  )
                ORDER BY created_at ASC
                LIMIT ?3
            ";

            let mut stmt = match conn.prepare(query) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };

            let rows = stmt.query_map(params![status, after, limit], row_to_record);
            match rows {
                Ok(mapped) => mapped.filter_map(Result::ok).collect(),
                Err(_) => Vec::new(),
            }
        })
        .await
        .unwrap_or_default()
    }

    pub async fn sweep_downloaded_older_than(&self, retention_secs: u64) -> usize {
        let db_path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = Connection::open(db_path).ok()?;
            let cutoff = (Utc::now() - chrono::Duration::seconds(retention_secs as i64)).to_rfc3339();

            let mut stmt = conn
                .prepare(
                    "
                    SELECT id, saved_path
                    FROM captures
                    WHERE downloaded_at IS NOT NULL
                      AND downloaded_at < ?
                    ",
                )
                .ok()?;

            let mut deleted = 0usize;
            let pairs = stmt
                .query_map(params![cutoff], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .ok()?;

            for pair in pairs.flatten() {
                let (id, saved_path) = pair;
                if let Some(path) = saved_path {
                    let _ = std::fs::remove_file(path);
                }
                if conn
                    .execute("DELETE FROM captures WHERE id = ?", params![id])
                    .is_ok()
                {
                    deleted += 1;
                }
            }

            Some(deleted)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or(0)
    }
}

fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CaptureRecord> {
    let status_text: String = row.get(1)?;
    let created_at_text: String = row.get(11)?;
    let started_at_text: Option<String> = row.get(12)?;
    let completed_at_text: Option<String> = row.get(13)?;
    let downloaded_at_text: Option<String> = row.get(14)?;

    Ok(CaptureRecord {
        id: row.get(0)?,
        status: status_from_str(&status_text),
        request_json: row.get(2)?,
        camera_model: row.get(3)?,
        saved_path: row.get(4)?,
        source_folder: row.get(5)?,
        source_name: row.get(6)?,
        size_bytes: row.get::<_, Option<i64>>(7)?.map(|v| v as u64),
        checksum: row.get(8)?,
        error: row.get(9)?,
        attempt_count: row.get::<_, i64>(10)? as u8,
        created_at: parse_datetime(&created_at_text),
        started_at: started_at_text.map(|v| parse_datetime(&v)),
        completed_at: completed_at_text.map(|v| parse_datetime(&v)),
        downloaded_at: downloaded_at_text.map(|v| parse_datetime(&v)),
    })
}

fn parse_datetime(value: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn status_to_str(status: CaptureStatus) -> String {
    match status {
        CaptureStatus::Queued => "queued",
        CaptureStatus::Capturing => "capturing",
        CaptureStatus::Downloading => "downloading",
        CaptureStatus::Complete => "complete",
        CaptureStatus::Failed => "failed",
        CaptureStatus::Canceled => "canceled",
    }
    .to_string()
}

fn status_from_str(status: &str) -> CaptureStatus {
    match status {
        "queued" => CaptureStatus::Queued,
        "capturing" => CaptureStatus::Capturing,
        "downloading" => CaptureStatus::Downloading,
        "complete" => CaptureStatus::Complete,
        "failed" => CaptureStatus::Failed,
        "canceled" => CaptureStatus::Canceled,
        _ => CaptureStatus::Failed,
    }
}

fn sha256_of_file(path: &str) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let read = file.read(&mut buf).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}
