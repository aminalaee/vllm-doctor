//! Integration tests for CLI commands (migrate, history list/show, diagnose --save).
//!
//! These exercise the command paths through the library API with a temp DB,
//! verifying the store round-trips and the CLI wiring produces the right output.
use vllm_doctor::config::Config;
use vllm_doctor::stores::{HistoryStore, SqliteHistoryStore};

fn test_config(dir: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.database.url = format!("sqlite://{}/history.db", dir.display());
    config
}

async fn save_sample(store: &SqliteHistoryStore) -> uuid::Uuid {
    use vllm_doctor::diagnosis::diagnose;
    use vllm_doctor::metrics::MetricSeriesSnapshot;
    use vllm_doctor::metrics::series::{MetricSample, MetricSeries};
    use vllm_doctor::providers::{Provider, ProviderError, ProviderMetadata};
    use vllm_doctor::rules::build_registry;

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

    let snapshot = MetricSeriesSnapshot {
        num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
        kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.95)]),
        ..Default::default()
    };
    let config = Config::default();
    let registry = build_registry(&config);
    let result = diagnose(&StubProvider(snapshot), &registry, "5m", None, &config)
        .await
        .unwrap();
    store.save(&result).await.unwrap()
}

#[tokio::test]
async fn migrate_creates_database() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    // The table exists after connect runs migrations.
    let runs = store.list().await.unwrap();
    assert!(runs.is_empty());
}

#[tokio::test]
async fn history_list_returns_empty_for_new_db() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    let runs = store.list().await.unwrap();
    assert!(runs.is_empty());
}

#[tokio::test]
async fn history_list_json_serializes_runs() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    save_sample(&store).await;
    let runs = store.list().await.unwrap();
    let json = serde_json::to_string(&runs).unwrap();
    assert!(json.contains("\"run_id\""));
    assert!(json.contains("\"health\""));
    assert!(json.contains("\"fired_count\""));
    assert!(json.contains("\"warning\"") || json.contains("\"critical\""));
}

#[tokio::test]
async fn history_show_returns_saved_run() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    let id = save_sample(&store).await;
    let result = store.get(&id.to_string()).await.unwrap();
    assert!(result.is_some());
    let result = result.unwrap();
    assert!(!result.checks.is_empty());
}

#[tokio::test]
async fn history_show_returns_none_for_unknown_id() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    let result = store.get(&uuid::Uuid::now_v7().to_string()).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn diagnose_save_persists_run() {
    use vllm_doctor::diagnosis::diagnose;
    use vllm_doctor::metrics::MetricSeriesSnapshot;
    use vllm_doctor::metrics::series::{MetricSample, MetricSeries};
    use vllm_doctor::providers::{Provider, ProviderError, ProviderMetadata};
    use vllm_doctor::rules::build_registry;

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

    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();

    let snapshot = MetricSeriesSnapshot {
        num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
        ..Default::default()
    };
    let registry = build_registry(&config);
    let result = diagnose(&StubProvider(snapshot), &registry, "5m", None, &config)
        .await
        .unwrap();

    let id = store.save(&result).await.unwrap();
    let loaded = store.get(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(loaded, result);
    let runs = store.list().await.unwrap();
    assert_eq!(runs.len(), 1);
}
