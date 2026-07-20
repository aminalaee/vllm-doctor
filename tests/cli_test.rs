//! Integration tests for CLI commands and their underlying persistence paths.
//!
//! The persistence tests use the library API with a temporary database. The
//! output and failure-path tests execute the compiled CLI against a mock server.
use vllm_doctor::config::Config;
use vllm_doctor::stores::{HistoryStore, SqliteHistoryStore};

fn test_config(dir: &std::path::Path) -> Config {
    let mut config = Config::default();
    config.database.url = format!("sqlite://{}/history.db", dir.display());
    config
}

fn write_test_config(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("vllm-doctor.toml");
    let config = test_config(dir);
    std::fs::write(
        &path,
        format!("[database]\nurl = \"{}\"\n", config.database.url),
    )
    .unwrap();
    path
}

async fn save_sample(store: &SqliteHistoryStore) -> uuid::Uuid {
    use vllm_doctor::diagnosis::diagnose;
    use vllm_doctor::metrics::MetricSeriesSnapshot;
    use vllm_doctor::metrics::series::{MetricSample, MetricSeries};
    use vllm_doctor::models::MetricsSource;
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
                metrics_source: MetricsSource::DirectScrape,
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
    let result = diagnose(
        &StubProvider(snapshot),
        &registry,
        "5m",
        None,
        &vllm_doctor::models::TargetMetadata::default(),
        &config,
    )
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
    assert!(json.contains("\"metrics_source\""));
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
    use vllm_doctor::models::MetricsSource;
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
                metrics_source: MetricsSource::DirectScrape,
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
    let result = diagnose(
        &StubProvider(snapshot),
        &registry,
        "5m",
        None,
        &vllm_doctor::models::TargetMetadata::default(),
        &config,
    )
    .await
    .unwrap();

    let id = store.save(&result).await.unwrap();
    let loaded = store.get(&id.to_string()).await.unwrap().unwrap();
    assert_eq!(loaded, result);
    let runs = store.list().await.unwrap();
    assert_eq!(runs.len(), 1);
}

#[tokio::test]
async fn cli_migrate_creates_database() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());

    let output = std::process::Command::new(binary_path())
        .args(["migrate", "--config", config_path.to_str().unwrap()])
        .output()
        .expect("failed to run vllm-doctor");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Database migrated successfully.\n"
    );
    let store = SqliteHistoryStore::connect(&test_config(dir.path()).database.url)
        .await
        .unwrap();
    assert!(store.list().await.unwrap().is_empty());
}

#[tokio::test]
async fn cli_history_list_json_returns_saved_run() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let config_path = write_test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    let run_id = save_sample(&store).await;

    let output = std::process::Command::new(binary_path())
        .args([
            "history",
            "list",
            "--config",
            config_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .output()
        .expect("failed to run vllm-doctor");

    assert_eq!(output.status.code(), Some(0));
    let runs: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(runs.as_array().unwrap().len(), 1);
    assert_eq!(runs[0]["run_id"], run_id.to_string());
}

#[tokio::test]
async fn cli_history_show_json_returns_saved_diagnosis() {
    let dir = tempfile::tempdir().unwrap();
    let config = test_config(dir.path());
    let config_path = write_test_config(dir.path());
    let store = SqliteHistoryStore::connect(&config.database.url)
        .await
        .unwrap();
    let run_id = save_sample(&store).await;

    let output = std::process::Command::new(binary_path())
        .args([
            "history",
            "show",
            &run_id.to_string(),
            "--config",
            config_path.to_str().unwrap(),
            "--output",
            "json",
        ])
        .output()
        .expect("failed to run vllm-doctor");

    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], "1");
    assert!(report["checks"].is_array());
}

#[tokio::test]
async fn cli_history_show_unknown_run_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = write_test_config(dir.path());
    let run_id = uuid::Uuid::now_v7().to_string();

    let output = std::process::Command::new(binary_path())
        .args([
            "history",
            "show",
            &run_id,
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run vllm-doctor");

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        format!("Error: run {run_id} not found.\n")
    );
}

fn binary_path() -> &'static str {
    env!("CARGO_BIN_EXE_vllm-doctor")
}

const SCRAPE_METRICS: &str = "# TYPE vllm:num_requests_running gauge\nvllm:num_requests_running 10.0\n# TYPE vllm:num_requests_waiting gauge\nvllm:num_requests_waiting 8.0\n";

async fn serve_scrape() -> wiremock::MockServer {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SCRAPE_METRICS))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn cli_json_output_ends_with_newline() {
    let server = serve_scrape().await;
    let url = format!("{}/metrics", server.uri());
    let output = std::process::Command::new(binary_path())
        .args(["diagnose", &url, "--output", "json"])
        .output()
        .expect("failed to run vllm-doctor");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.ends_with('\n'), "JSON output must end with newline");
    assert!(stdout.contains("\"health\""));
}

#[tokio::test]
async fn cli_text_output_contains_health() {
    let server = serve_scrape().await;
    let url = format!("{}/metrics", server.uri());
    let output = std::process::Command::new(binary_path())
        .args(["diagnose", &url])
        .output()
        .expect("failed to run vllm-doctor");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Health:"));
}

#[tokio::test]
async fn cli_save_failure_prints_output_and_exits_2() {
    let server = serve_scrape().await;
    let url = format!("{}/metrics", server.uri());
    let dir = tempfile::tempdir().unwrap();
    let db_file = dir.path().join("blocker_file");
    std::fs::write(&db_file, "not a directory").unwrap();
    let config_path = dir.path().join("bad-config.toml");
    let db_url = format!("sqlite://{}/history.db", db_file.display());
    std::fs::write(&config_path, format!("[database]\nurl = \"{db_url}\"\n")).unwrap();

    let output = std::process::Command::new(binary_path())
        .args([
            "diagnose",
            &url,
            "--save",
            "--config",
            config_path.to_str().unwrap(),
        ])
        .output()
        .expect("failed to run vllm-doctor");

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        !stdout.is_empty(),
        "diagnosis output must appear even when save fails"
    );
    assert!(stdout.contains("Health:"));
    assert!(
        stderr.to_lowercase().contains("save") || stderr.to_lowercase().contains("error"),
        "stderr should report the save failure, got: {stderr}"
    );
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(unix)]
#[tokio::test]
async fn cli_watch_recovers_when_target_starts_late_and_stops_on_sigint() {
    use std::process::Stdio;
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let first_attempt_listener = listener.try_clone().unwrap();
    let first_attempt = std::thread::spawn(move || {
        let (stream, _) = first_attempt_listener.accept().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        drop(stream);
    });

    let url = format!("http://{address}/metrics");
    let child = std::process::Command::new(binary_path())
        .args([
            "diagnose",
            &url,
            "--watch",
            "--interval",
            "0.05",
            "--timeout",
            "0.1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run vllm-doctor");

    first_attempt.join().unwrap();
    let server = MockServer::builder().listener(listener).start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SCRAPE_METRICS))
        .mount(&server)
        .await;

    tokio::time::sleep(Duration::from_secs(2)).await;
    let signal_status = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("failed to send SIGINT");
    assert!(signal_status.success());

    let output = child
        .wait_with_output()
        .expect("watch process did not exit");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout.contains("Health:"), "stdout was: {stdout}");
    assert!(
        stderr.contains("provider setup failed"),
        "stderr was: {stderr}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cli_watch_serves_observability_endpoints_and_stops_cleanly() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let metrics_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metrics"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SCRAPE_METRICS))
        .mount(&metrics_server)
        .await;

    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let listen = format!("127.0.0.1:{port}");
    let child = std::process::Command::new(binary_path())
        .args([
            "diagnose",
            &format!("{}/metrics", metrics_server.uri()),
            "--watch",
            "--interval",
            "0.05",
            "--listen",
            &listen,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to run vllm-doctor");

    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(response) = client.get(format!("http://{listen}/readyz")).send().await {
            if response.status().is_success() {
                break;
            }
        }
        assert!(
            Instant::now() < deadline,
            "observability server did not become ready"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    let health = client
        .get(format!("http://{listen}/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(health.status(), reqwest::StatusCode::OK);
    assert_eq!(health.text().await.unwrap(), "ok\n");
    let metrics = client
        .get(format!("http://{listen}/metrics"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(metrics.contains("vllm_doctor_ready{target=\"unconfigured\",engine=\"vllm\"} 1"));

    let signal_status = std::process::Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("failed to send SIGTERM");
    assert!(signal_status.success());
    let output = child
        .wait_with_output()
        .expect("watch process did not exit");
    assert_eq!(output.status.code(), Some(0));
}
