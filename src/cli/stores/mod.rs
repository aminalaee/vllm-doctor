//! Persistence for diagnosis runs, behind a storage interface so a future
//! server mode can swap the backend (local SQLite now; see v0.6 storage plan).
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::core::models::{DiagnosisResult, Health, InferenceEngine, MetricsSource};

pub mod sqlite;
pub use sqlite::SqliteHistoryStore;

/// Version of the serialized report payload. This remains at 1 while the
/// pre-release schema is allowed to change in place; `get` rejects any other
/// stored version before deserializing.
pub const REPORT_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("report serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error(
        "run {id} was saved by an incompatible report format (stored v{stored}, current v{current})"
    )]
    IncompatibleReport { id: Uuid, stored: i64, current: i64 },
}

/// Summary of a stored run, for listing without loading the full report. Built
/// from denormalized columns so it stays readable even if the report blob is an
/// incompatible version.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[must_use]
pub struct RunSummary {
    pub run_id: Uuid,
    pub saved_at: DateTime<Utc>,
    pub model_name: Option<String>,
    pub metrics_source: MetricsSource,
    pub engine: InferenceEngine,
    pub target_id: Option<String>,
    pub health: Health,
    pub fired_count: i64,
}

/// A store of diagnosis history.
#[async_trait::async_trait]
pub trait HistoryStore {
    /// Persist a run; returns its generated id.
    async fn save(&self, result: &DiagnosisResult) -> Result<Uuid, StoreError>;

    /// List stored runs, newest first.
    async fn list(&self) -> Result<Vec<RunSummary>, StoreError>;

    /// Load a full run by id. Returns `None` for an unknown or malformed id, and
    /// an error if the stored report format is incompatible with this build.
    async fn get(&self, run_id: &str) -> Result<Option<DiagnosisResult>, StoreError>;
}
