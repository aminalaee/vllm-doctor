//! Golden and adversarial contract tests for `ObservationV1`.
//!
//! The golden test pins a fixed-input observation to a committed JSON fixture. The
//! adversarial tests assert that arbitrary Prometheus labels, raw replica
//! names, endpoint credentials, and diagnosis prose cannot enter the payload.

use chrono::{DateTime, Utc};
use serde_json::Value;
use vllm_doctor::config::Config;
use vllm_doctor::diagnosis::diagnose;
use vllm_doctor::metrics::series::MetricSample;
use vllm_doctor::metrics::{
    Aggregate, MetricSeries, MetricSeriesSnapshot, ObservationUnit, all_specs,
};
use vllm_doctor::models::{MetricsSource, TargetMetadata};
use vllm_doctor::observations::parse_window_seconds;
use vllm_doctor::observations::v1::{
    AvailabilityStatusV1, MeasurementKindV1, ObservationBuildContext, ObservationBuildError,
    build_observation,
};
use vllm_doctor::providers::{Provider, ProviderError, ProviderMetadata};
use vllm_doctor::rules::build_registry;

/// A provider that always returns the same snapshot — used to build a
/// deterministic `DiagnosisResult`.
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

fn sample(value: f64, labels: &[(&str, &str)]) -> MetricSample {
    MetricSample {
        labels: labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        value,
        timestamp: None,
    }
}

/// Snapshot with two replicas and known metric values.
fn golden_snapshot() -> MetricSeriesSnapshot {
    MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![sample(10.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
            aggregate_by: Aggregate::Sum,
        },
        kv_cache_usage_perc: MetricSeries {
            samples: vec![sample(0.5, &[("pod", "a")]), sample(0.9, &[("pod", "b")])],
            aggregate_by: Aggregate::Max,
        },
        ..Default::default()
    }
}

fn fixed_ctx() -> ObservationBuildContext {
    ObservationBuildContext {
        event_id: "01923f5c-3e8e-7c8e-9d4f-0123456789ab".parse().unwrap(),
        observed_at: DateTime::parse_from_rfc3339("2026-07-21T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
        agent_id: "agent-local-dev".to_string(),
        agent_version: vllm_doctor::version().to_string(),
        local_rule_pack: vllm_doctor::version().to_string(),
    }
}

async fn golden_result() -> vllm_doctor::models::DiagnosisResult {
    let snapshot = golden_snapshot();
    let config = Config::default();
    let registry = build_registry(&config);
    let target = TargetMetadata {
        id: Some("prod-llama-70b".to_string()),
        engine: vllm_doctor::models::InferenceEngine::Vllm,
        engine_version: Some("0.10.0".to_string()),
        environment: Some("production".to_string()),
    };
    diagnose(
        &StubProvider(snapshot),
        &registry,
        "5m",
        Some("meta-llama/Llama-3.1-70B-Instruct"),
        &target,
        &config,
    )
    .await
    .unwrap()
}

fn fixture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("contracts")
        .join("observation-v1.json")
}

#[tokio::test]
async fn golden_batch_matches_fixture() {
    let result = golden_result().await;
    let window = parse_window_seconds("5m").unwrap();
    let batch = build_observation(&result, &fixed_ctx(), window).unwrap();
    let pretty = serde_json::to_string_pretty(&batch).unwrap();
    let fixture = std::fs::read_to_string(fixture_path()).unwrap_or_default();
    assert_eq!(pretty, fixture, "golden batch JSON does not match fixture");
}

/// Helper to build a batch from a snapshot and pretty-serialize it.
async fn build_batch_from_snapshot(
    snapshot: MetricSeriesSnapshot,
    target_id: &str,
) -> vllm_doctor::observations::v1::ObservationV1 {
    build_batch_from_snapshot_with_source(snapshot, target_id, MetricsSource::DirectScrape).await
}

async fn build_batch_from_snapshot_with_source(
    snapshot: MetricSeriesSnapshot,
    target_id: &str,
    source: MetricsSource,
) -> vllm_doctor::observations::v1::ObservationV1 {
    let config = Config::default();
    let registry = build_registry(&config);
    let target = TargetMetadata {
        id: Some(target_id.to_string()),
        engine: vllm_doctor::models::InferenceEngine::Vllm,
        ..Default::default()
    };
    let mut result = diagnose(
        &StubProvider(snapshot),
        &registry,
        "5m",
        None,
        &target,
        &config,
    )
    .await
    .unwrap();
    result.context.metrics_source = source;
    build_observation(&result, &fixed_ctx(), 300).unwrap()
}

/// Snapshot with forbidden labels to verify they never appear in the payload.
fn malicious_snapshot() -> MetricSeriesSnapshot {
    MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![
                sample(
                    10.0,
                    &[
                        ("pod", "private-customer-hostname"),
                        ("authorization", "Bearer secret"),
                        ("prompt", "customer prompt"),
                        ("request_id", "req-123"),
                        ("endpoint", "https://user:password@example.invalid/metrics"),
                        ("model_name", "secret-model"),
                    ],
                ),
                sample(
                    2.0,
                    &[
                        ("pod", "other-private-host"),
                        ("prompt", "another customer prompt"),
                    ],
                ),
            ],
            aggregate_by: Aggregate::Sum,
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn forbidden_labels_never_appear_in_json() {
    let batch = build_batch_from_snapshot(malicious_snapshot(), "prod").await;
    let compact = serde_json::to_string(&batch).unwrap();
    let pretty = serde_json::to_string_pretty(&batch).unwrap();
    // Forbidden label keys must never appear as JSON keys. Check the quoted
    // form so that substrings of valid observation ids (e.g. `prompt` inside
    // `prompt_token_throughput`) do not produce false positives.
    for forbidden_key in [
        "\"authorization\"",
        "\"prompt\"",
        "\"request_id\"",
        "\"endpoint\"",
        "\"pod\"",
        "\"model_name\"",
    ] {
        assert!(
            !compact.contains(forbidden_key),
            "compact JSON contains forbidden key `{forbidden_key}`"
        );
        assert!(
            !pretty.contains(forbidden_key),
            "pretty JSON contains forbidden key `{forbidden_key}`"
        );
    }
    // Forbidden label values and raw replica names must never appear either.
    for forbidden in [
        "Bearer secret",
        "customer prompt",
        "another customer prompt",
        "req-123",
        "https://user:password@example.invalid/metrics",
        "private-customer-hostname",
        "other-private-host",
        "secret-model",
    ] {
        assert!(
            !compact.contains(forbidden),
            "compact JSON contains forbidden `{forbidden}`"
        );
        assert!(
            !pretty.contains(forbidden),
            "pretty JSON contains forbidden `{forbidden}`"
        );
    }
}

#[tokio::test]
async fn only_stable_registry_ids_occur() {
    let batch = build_batch_from_snapshot(golden_snapshot(), "prod-llama-70b").await;
    let json = serde_json::to_string(&batch).unwrap();
    // No vllm: prefixed metric names.
    assert!(!json.contains("vllm:"));
    // No Rust field names (snake_case snapshot fields) that are NOT also
    // registry ids. `prefix_cache_hit_rate` is both a Rust field name and a
    // valid registry observation id, so it is intentionally excluded here.
    for rust_field in [
        "num_requests_running",
        "kv_cache_usage_perc",
        "prompt_tokens_per_second",
        "generation_tokens_per_second",
        "request_success_total",
        "request_error_total",
        "request_abort_total",
        "ttft_p95_seconds",
        "tpot_p95_seconds",
        "queue_time_p95_seconds",
        "num_preemptions_total",
    ] {
        assert!(
            !json.contains(&format!("\"{rust_field}\"")),
            "payload contains Rust field name `{rust_field}`"
        );
    }
    // Every observation id is one of the registry ids.
    let valid_ids: std::collections::HashSet<&str> = all_specs()
        .iter()
        .map(|s| s.observation_spec().id)
        .collect();
    let payload: Value = serde_json::from_str(&json).unwrap();
    for obs in payload["observations"].as_array().unwrap() {
        let id = obs["id"].as_str().unwrap();
        assert!(
            valid_ids.contains(id),
            "observation id `{id}` is not a registered id"
        );
    }
    for avail in payload["availability"].as_array().unwrap() {
        let id = avail["id"].as_str().unwrap();
        assert!(
            valid_ids.contains(id),
            "availability id `{id}` is not a registered id"
        );
    }
}

#[tokio::test]
async fn missing_values_produce_availability_not_zeros() {
    // Snapshot with no gauge values filled; only engine fields default.
    let snapshot = MetricSeriesSnapshot::default();
    let batch = build_batch_from_snapshot(snapshot, "prod").await;
    // No observation value should be 0.0 emitted as a fallback for missing data.
    let json = serde_json::to_string(&batch).unwrap();
    assert!(!json.contains("\"value\":0.0"));
    // Availability should be non-empty.
    assert!(!batch.availability.is_empty());
    for avail in &batch.availability {
        assert_eq!(avail.status, AvailabilityStatusV1::NotCollected);
    }
}

#[tokio::test]
async fn counter_increases_labeled_window_delta() {
    // request_success_total is an Increase probe -> WindowDelta kind.
    let snapshot = MetricSeriesSnapshot {
        request_success_total: MetricSeries::scalar(42.0),
        ..Default::default()
    };
    let batch =
        build_batch_from_snapshot_with_source(snapshot, "prod", MetricsSource::Prometheus).await;
    let succeeded = batch
        .observations
        .iter()
        .find(|m| m.id == "requests_succeeded")
        .expect("requests_succeeded observation present");
    assert_eq!(succeeded.kind, MeasurementKindV1::WindowDelta);
}

#[tokio::test]
async fn quantiles_include_quantile_095() {
    let snapshot = MetricSeriesSnapshot {
        ttft_p95_seconds: MetricSeries::scalar(3.2),
        ..Default::default()
    };
    let batch =
        build_batch_from_snapshot_with_source(snapshot, "prod", MetricsSource::Prometheus).await;
    let ttft = batch
        .observations
        .iter()
        .find(|m| m.id == "time_to_first_token")
        .expect("time_to_first_token observation present");
    assert_eq!(ttft.quantile, Some(0.95));
}

#[tokio::test]
async fn direct_scrape_does_not_export_promql_derived_values() {
    let snapshot = MetricSeriesSnapshot {
        request_success_total: MetricSeries::scalar(42.0),
        ttft_p95_seconds: MetricSeries::scalar(3.2),
        prefix_cache_hit_rate: MetricSeries::scalar(0.75),
        ..Default::default()
    };
    let batch = build_batch_from_snapshot(snapshot, "prod").await;

    for id in [
        "requests_succeeded",
        "time_to_first_token",
        "prefix_cache_hit_rate",
    ] {
        assert!(
            batch
                .observations
                .iter()
                .all(|measurement| measurement.id != id),
            "direct scrape exported `{id}` with PromQL-only semantics"
        );
        assert!(batch.availability.iter().any(|availability| {
            availability.id == id && availability.status == AvailabilityStatusV1::NotCollected
        }));
    }
}

#[tokio::test]
async fn replica_aliases_are_deterministic() {
    let batch1 = build_batch_from_snapshot(golden_snapshot(), "prod-llama-70b").await;
    let batch2 = build_batch_from_snapshot(golden_snapshot(), "prod-llama-70b").await;
    assert_eq!(
        serde_json::to_string(&batch1).unwrap(),
        serde_json::to_string(&batch2).unwrap()
    );
    // Aliases must be replica-1, replica-2.
    let aliases: Vec<&str> = batch1
        .observations
        .iter()
        .filter_map(|m| m.dimensions.as_ref().map(|d| d.replica.as_str()))
        .collect();
    assert!(aliases.iter().all(|a| a.starts_with("replica-")));
}

#[tokio::test]
async fn reordering_input_samples_does_not_change_output() {
    let snapshot_a = MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![sample(10.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
            aggregate_by: Aggregate::Sum,
        },
        kv_cache_usage_perc: MetricSeries {
            samples: vec![sample(0.5, &[("pod", "a")]), sample(0.9, &[("pod", "b")])],
            aggregate_by: Aggregate::Max,
        },
        ..Default::default()
    };
    let snapshot_b = MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![sample(2.0, &[("pod", "b")]), sample(10.0, &[("pod", "a")])],
            aggregate_by: Aggregate::Sum,
        },
        kv_cache_usage_perc: MetricSeries {
            samples: vec![sample(0.9, &[("pod", "b")]), sample(0.5, &[("pod", "a")])],
            aggregate_by: Aggregate::Max,
        },
        ..Default::default()
    };
    let batch_a = build_batch_from_snapshot(snapshot_a, "prod-llama-70b").await;
    let batch_b = build_batch_from_snapshot(snapshot_b, "prod-llama-70b").await;
    assert_eq!(
        serde_json::to_string(&batch_a).unwrap(),
        serde_json::to_string(&batch_b).unwrap(),
        "reordering input samples changed the serialized batch"
    );
}

#[tokio::test]
async fn more_than_64_replicas_fails() {
    let mut samples = Vec::new();
    for i in 0..65 {
        samples.push(sample(1.0, &[("pod", &format!("r{i}"))]));
    }
    let snapshot = MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples,
            aggregate_by: Aggregate::Sum,
        },
        ..Default::default()
    };
    let config = Config::default();
    let registry = build_registry(&config);
    let target = TargetMetadata {
        id: Some("prod".to_string()),
        ..Default::default()
    };
    let result = diagnose(
        &StubProvider(snapshot),
        &registry,
        "5m",
        None,
        &target,
        &config,
    )
    .await
    .unwrap();
    let err = build_observation(&result, &fixed_ctx(), 300).unwrap_err();
    assert!(matches!(err, ObservationBuildError::TooManyReplicas));
}

#[test]
fn disallowed_metrics_do_not_affect_replica_selection_or_limits() {
    let disallowed_samples = (0..65)
        .map(|i| sample(1.0, &[("pod", &format!("private-{i}"))]))
        .collect();
    let snapshot = MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![sample(10.0, &[("pod", "a")]), sample(2.0, &[("pod", "b")])],
            aggregate_by: Aggregate::Sum,
        },
        request_success_total: MetricSeries {
            samples: disallowed_samples,
            aggregate_by: Aggregate::Sum,
        },
        ..Default::default()
    };
    let context = vllm_doctor::models::DiagnosisContext::new("5m")
        .with_metrics_source(MetricsSource::Prometheus)
        .with_target(TargetMetadata {
            id: Some("prod".to_string()),
            ..Default::default()
        });
    let result = vllm_doctor::models::DiagnosisResult::new(context, snapshot, vec![]);
    let batch = build_observation(&result, &fixed_ctx(), 300).unwrap();

    let aliases: std::collections::HashSet<&str> = batch
        .observations
        .iter()
        .filter_map(|measurement| {
            measurement
                .dimensions
                .as_ref()
                .map(|dimensions| dimensions.replica.as_str())
        })
        .collect();
    assert_eq!(
        aliases,
        std::collections::HashSet::from(["replica-1", "replica-2"])
    );
    assert!(batch.observations.iter().all(|measurement| {
        measurement.id != "requests_succeeded" || measurement.dimensions.is_none()
    }));
}

#[tokio::test]
async fn blank_agent_and_target_ids_fail_at_builder_boundary() {
    let result = golden_result().await;
    let blank_agent = ObservationBuildContext {
        agent_id: "   ".to_string(),
        ..fixed_ctx()
    };
    assert!(matches!(
        build_observation(&result, &blank_agent, 300),
        Err(ObservationBuildError::InvalidAgentId)
    ));

    let mut blank_target = result.clone();
    blank_target.context.target.id = Some("".to_string());
    assert!(matches!(
        build_observation(&blank_target, &fixed_ctx(), 300),
        Err(ObservationBuildError::InvalidTargetId)
    ));

    let mut missing_target = blank_target;
    missing_target.context.target.id = None;
    assert!(matches!(
        build_observation(&missing_target, &fixed_ctx(), 300),
        Err(ObservationBuildError::MissingTargetId)
    ));

    assert!(matches!(
        build_observation(&result, &fixed_ctx(), 0),
        Err(ObservationBuildError::InvalidWindow(_))
    ));
}

#[tokio::test]
async fn firing_rule_ids_are_sorted() {
    let mut result = golden_result().await;
    result.checks = ["zeta", "alpha"]
        .into_iter()
        .map(|id| vllm_doctor::models::RuleResult {
            id: id.to_string(),
            name: id.to_string(),
            title: id.to_string(),
            severity: vllm_doctor::models::Severity::Warning,
            finding: Some(vllm_doctor::models::Finding {
                severity: vllm_doctor::models::Severity::Warning,
                confidence: vllm_doctor::models::Confidence::High,
                title: id.to_string(),
                signals: vec![],
                evidence: vec![],
                likely_causes: vec![],
                recommendations: vec![],
                related_metrics: vec![],
            }),
        })
        .collect();
    let batch = build_observation(&result, &fixed_ctx(), 300).unwrap();
    assert_eq!(batch.local_diagnosis.firing_rule_ids, ["alpha", "zeta"]);
}

#[tokio::test]
async fn overlong_ids_fail() {
    let result = golden_result().await;
    let long_ctx = ObservationBuildContext {
        agent_id: "x".repeat(129),
        ..fixed_ctx()
    };
    let err = build_observation(&result, &long_ctx, 300).unwrap_err();
    assert!(matches!(err, ObservationBuildError::IdentifierTooLong));
}

#[tokio::test]
async fn non_finite_values_never_serialize() {
    let snapshot = MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![sample(f64::NAN, &[("pod", "a")])],
            aggregate_by: Aggregate::Sum,
        },
        ..Default::default()
    };
    let batch = build_batch_from_snapshot(snapshot, "prod").await;
    let compact = serde_json::to_string(&batch).unwrap();
    let pretty = serde_json::to_string_pretty(&batch).unwrap();
    for forbidden in ["NaN", "nan", "inf", "-inf", "Infinity", "\"NaN\""] {
        assert!(
            !compact.contains(forbidden),
            "compact contains `{forbidden}`"
        );
        assert!(!pretty.contains(forbidden), "pretty contains `{forbidden}`");
    }
    // requests_running should be in availability, not observations.
    assert!(
        batch
            .availability
            .iter()
            .any(|a| a.id == "requests_running")
    );
}

#[tokio::test]
async fn compact_payload_cannot_exceed_256_kib() {
    let batch = build_batch_from_snapshot(golden_snapshot(), "prod-llama-70b").await;
    let bytes = serde_json::to_vec(&batch).unwrap();
    assert!(bytes.len() <= 256 * 1024);
}

#[tokio::test]
async fn no_diagnosis_prose_in_json() {
    let mut result = golden_result().await;
    result.assessment = vllm_doctor::models::Assessment {
        likely_bottleneck: vllm_doctor::models::BottleneckKind::ReplicaImbalance,
        confidence: vllm_doctor::models::Confidence::High,
        evidence: vec![vllm_doctor::models::EvidenceItem::text(
            "secret evidence prose",
        )],
        interpretation: "secret interpretation prose".to_string(),
        recommended_next_actions: vec!["secret recommendation prose".to_string()],
    };
    result.checks.push(vllm_doctor::models::RuleResult {
        id: "fake".into(),
        name: "Fake".into(),
        title: "secret finding title".into(),
        severity: vllm_doctor::models::Severity::Warning,
        finding: Some(vllm_doctor::models::Finding {
            severity: vllm_doctor::models::Severity::Warning,
            confidence: vllm_doctor::models::Confidence::High,
            title: "secret finding title".into(),
            signals: vec!["secret signal".into()],
            evidence: vec![vllm_doctor::models::EvidenceItem::text("secret evidence")],
            likely_causes: vec!["secret likely cause".into()],
            recommendations: vec!["secret recommendation".into()],
            related_metrics: vec!["vllm:secret_metric".into()],
        }),
    });
    let batch = build_observation(&result, &fixed_ctx(), 300).unwrap();
    let json = serde_json::to_string(&batch).unwrap();
    for forbidden in [
        "secret evidence prose",
        "secret interpretation prose",
        "secret recommendation prose",
        "secret finding title",
        "secret signal",
        "secret evidence",
        "secret likely cause",
        "secret recommendation",
        "vllm:secret_metric",
        "interpretation",
        "recommendations",
        "likely_causes",
        "evidence",
    ] {
        assert!(
            !json.contains(forbidden),
            "payload contains diagnosis prose `{forbidden}`"
        );
    }
}

/// Sanity check that the registry exposes a `count` unit, ensuring the
/// `ObservationUnit` mapping is exercised by the golden fixture.
#[test]
fn registry_exposes_count_unit() {
    let has_count = all_specs()
        .iter()
        .any(|s| s.observation_spec().unit == ObservationUnit::Count);
    assert!(has_count);
}
