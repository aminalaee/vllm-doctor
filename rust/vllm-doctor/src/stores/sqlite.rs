//! SQLite-backed history store.
use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use uuid::Uuid;

use super::{HistoryStore, REPORT_VERSION, RunSummary, StoreError};
use crate::models::{ClientMode, DiagnosisResult, Health};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/stores/migrations");

pub struct SqliteHistoryStore {
    pool: SqlitePool,
}

impl SqliteHistoryStore {
    /// Open the database (creating the file if needed) and apply migrations.
    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        let options = if url == "sqlite:///:memory:" {
            SqliteConnectOptions::new().in_memory(true)
        } else {
            SqliteConnectOptions::from_str(url)?.create_if_missing(true)
        };
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }
}

#[async_trait::async_trait]
impl HistoryStore for SqliteHistoryStore {
    async fn save(&self, result: &DiagnosisResult) -> Result<Uuid, StoreError> {
        let id = Uuid::now_v7();
        let saved_at = Utc::now();
        let fired_count = result.checks.iter().filter(|c| c.finding.is_some()).count() as i64;
        let report = serde_json::to_string(result)?;

        sqlx::query(
            "INSERT INTO runs \
             (id, saved_at, model_name, client_mode, health, fired_count, report_version, report) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(saved_at)
        .bind(result.context.model_name.clone())
        .bind(result.context.client_mode.to_string())
        .bind(result.health().to_string())
        .bind(fired_count)
        .bind(REPORT_VERSION)
        .bind(report)
        .execute(&self.pool)
        .await?;

        Ok(id)
    }

    async fn list(&self) -> Result<Vec<RunSummary>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, saved_at, model_name, client_mode, health, fired_count \
             FROM runs ORDER BY saved_at DESC, id DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut summaries = Vec::with_capacity(rows.len());
        for row in rows {
            let client_mode: String = row.try_get("client_mode")?;
            let health: String = row.try_get("health")?;
            summaries.push(RunSummary {
                run_id: row.try_get("id")?,
                saved_at: row.try_get::<DateTime<Utc>, _>("saved_at")?,
                model_name: row.try_get("model_name")?,
                client_mode: ClientMode::from_str(&client_mode).unwrap_or_default(),
                health: Health::from_str(&health).unwrap_or(Health::Ok),
                fired_count: row.try_get("fired_count")?,
            });
        }
        Ok(summaries)
    }

    async fn get(&self, run_id: &str) -> Result<Option<DiagnosisResult>, StoreError> {
        let Ok(id) = Uuid::parse_str(run_id) else {
            return Ok(None);
        };
        let row = sqlx::query("SELECT report_version, report FROM runs WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };

        let stored: i64 = row.try_get("report_version")?;
        if stored != REPORT_VERSION {
            return Err(StoreError::IncompatibleReport {
                id,
                stored,
                current: REPORT_VERSION,
            });
        }
        let report: String = row.try_get("report")?;
        Ok(Some(serde_json::from_str(&report)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::diagnosis::diagnose;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::{ClientMode, DiagnosisContext, DiagnosisResult};
    use crate::providers::{Provider, ProviderError, ProviderMetadata};
    use crate::rules::build_registry;

    struct StubProvider(MetricSeriesSnapshot);

    #[async_trait::async_trait]
    impl Provider for StubProvider {
        async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
            Ok(self.0.clone())
        }
        fn metadata(&self) -> ProviderMetadata {
            ProviderMetadata {
                id: "scrape",
                endpoint: "test".into(),
            }
        }
    }

    async fn store() -> (tempfile::TempDir, SqliteHistoryStore) {
        let dir = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", dir.path().join("history.db").display());
        let store = SqliteHistoryStore::connect(&url).await.unwrap();
        (dir, store)
    }

    fn sample_result() -> DiagnosisResult {
        let snapshot = MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.95)]),
            ..Default::default()
        };
        let registry = build_registry(&Config::default());
        DiagnosisResult {
            context: DiagnosisContext::new("5m")
                .with_client_mode(ClientMode::Scrape)
                .with_model_name("llama"),
            checks: registry.run_all(&snapshot),
            metric_series: snapshot,
        }
    }

    #[tokio::test]
    async fn save_then_get_round_trips() {
        let (_dir, store) = store().await;
        let result = sample_result();
        let id = store.save(&result).await.unwrap();
        let loaded = store.get(&id.to_string()).await.unwrap().unwrap();
        assert_eq!(loaded, result);
    }

    #[tokio::test]
    async fn list_summarizes_newest_first() {
        let (_dir, store) = store().await;
        store.save(&sample_result()).await.unwrap();
        store.save(&sample_result()).await.unwrap();
        let runs = store.list().await.unwrap();
        assert_eq!(runs.len(), 2);
        assert!(runs[0].saved_at >= runs[1].saved_at);
        assert_eq!(runs[0].client_mode, ClientMode::Scrape);
        assert_eq!(runs[0].model_name, Some("llama".to_string()));
        assert!(runs[0].fired_count >= 1);
    }

    #[tokio::test]
    async fn get_returns_none_for_unknown_or_malformed_id() {
        let (_dir, store) = store().await;
        assert!(store.get("not-a-uuid").await.unwrap().is_none());
        assert!(
            store
                .get(&Uuid::now_v7().to_string())
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn incompatible_version_errors_on_get_but_list_survives() {
        let (_dir, store) = store().await;
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO runs \
             (id, saved_at, model_name, client_mode, health, fired_count, report_version, report) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(Utc::now())
        .bind(None::<String>)
        .bind("scrape")
        .bind("warning")
        .bind(1_i64)
        .bind(REPORT_VERSION + 1)
        .bind("{}")
        .execute(&store.pool)
        .await
        .unwrap();

        // list reads only summary columns, so it still works.
        assert_eq!(store.list().await.unwrap().len(), 1);
        // get refuses the incompatible blob with a clear error.
        let err = store.get(&id.to_string()).await.unwrap_err();
        assert!(matches!(err, StoreError::IncompatibleReport { .. }));
    }

    #[tokio::test]
    async fn non_finite_sample_run_is_readable() {
        let (_dir, store) = store().await;
        let mut result = sample_result();
        result.metric_series.kv_cache_usage_perc =
            MetricSeries::from_samples(vec![MetricSample::new(f64::INFINITY)]);
        let id = store.save(&result).await.unwrap();
        let loaded = store.get(&id.to_string()).await.unwrap().unwrap();
        assert!(
            loaded.metric_series.kv_cache_usage_perc.samples[0]
                .value
                .is_infinite()
        );
    }

    #[tokio::test]
    async fn diagnose_result_persists_end_to_end() {
        let (_dir, store) = store().await;
        let snapshot = MetricSeriesSnapshot {
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.95)]),
            ..Default::default()
        };
        let registry = build_registry(&Config::default());
        let result = diagnose(&StubProvider(snapshot), &registry, "5m", None)
            .await
            .unwrap();
        let id = store.save(&result).await.unwrap();
        assert_eq!(store.get(&id.to_string()).await.unwrap().unwrap(), result);
    }
}
