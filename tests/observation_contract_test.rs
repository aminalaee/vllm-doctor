//! Golden and adversarial contract tests for `ObservationV1`.
//!
//! The golden test pins a fixed-input observation to a committed JSON fixture. The
//! adversarial tests assert that arbitrary Prometheus labels, raw replica
//! names, endpoint credentials, and diagnosis prose cannot enter the payload.

use chrono::{DateTime, Utc};
use serde_json::Value;
use vllm_doctor::core::config::CoreConfig as Config;
use vllm_doctor::core::diagnosis::{diagnose, diagnose_snapshot};
use vllm_doctor::core::metrics::series::MetricSample;
use vllm_doctor::core::metrics::{
    Aggregate, MetricSeries, MetricSeriesSnapshot, ObservationUnit, all_specs,
};
use vllm_doctor::core::models::{MetricsSource, TargetMetadata};
use vllm_doctor::core::observations::parse_window_seconds;
use vllm_doctor::core::observations::v1::{
    AvailabilityStatusV1, MeasurementKindV1, MetricsSourceV1, ObservationBuildContext,
    ObservationBuildError, ObservationV1, ObservationValidationError, build_observation,
    diagnosis_context, reconstruct_snapshot, validate_observation,
};
use vllm_doctor::core::providers::{Provider, ProviderError, ProviderMetadata};
use vllm_doctor::core::rules::build_registry;

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
            metrics_source: MetricsSource::Prometheus,
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

async fn golden_result() -> vllm_doctor::core::models::DiagnosisResult {
    let snapshot = golden_snapshot();
    let config = Config::default();
    let registry = build_registry(&config);
    let target = TargetMetadata {
        id: Some("prod-llama-70b".to_string()),
        engine: vllm_doctor::core::models::InferenceEngine::Vllm,
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
    assert_eq!(
        pretty,
        fixture.trim_end(),
        "golden batch JSON does not match fixture"
    );
}

/// Helper to build a batch from a snapshot and pretty-serialize it.
async fn build_batch_from_snapshot(
    snapshot: MetricSeriesSnapshot,
    target_id: &str,
) -> vllm_doctor::core::observations::v1::ObservationV1 {
    build_batch_from_snapshot_with_source(snapshot, target_id, MetricsSource::DirectScrape).await
}

async fn build_batch_from_snapshot_with_source(
    snapshot: MetricSeriesSnapshot,
    target_id: &str,
    source: MetricsSource,
) -> vllm_doctor::core::observations::v1::ObservationV1 {
    let config = Config::default();
    let registry = build_registry(&config);
    let target = TargetMetadata {
        id: Some(target_id.to_string()),
        engine: vllm_doctor::core::models::InferenceEngine::Vllm,
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
    assert_eq!(batch.metrics_source, MetricsSourceV1::DirectScrape);
    assert_eq!(
        diagnosis_context(&batch).metrics_source,
        MetricsSource::DirectScrape
    );

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
    let context = vllm_doctor::core::models::DiagnosisContext::new("5m")
        .with_metrics_source(MetricsSource::Prometheus)
        .with_target(TargetMetadata {
            id: Some("prod".to_string()),
            ..Default::default()
        });
    let result = vllm_doctor::core::models::DiagnosisResult::new(context, snapshot, vec![]);
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
        .map(|id| vllm_doctor::core::models::RuleResult {
            id: id.to_string(),
            name: id.to_string(),
            title: id.to_string(),
            severity: vllm_doctor::core::models::Severity::Warning,
            finding: Some(vllm_doctor::core::models::Finding {
                severity: vllm_doctor::core::models::Severity::Warning,
                confidence: vllm_doctor::core::models::Confidence::High,
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
    result.assessment = vllm_doctor::core::models::Assessment {
        likely_bottleneck: vllm_doctor::core::models::BottleneckKind::ReplicaImbalance,
        confidence: vllm_doctor::core::models::Confidence::High,
        evidence: vec![vllm_doctor::core::models::EvidenceItem::text(
            "secret evidence prose",
        )],
        interpretation: "secret interpretation prose".to_string(),
        recommended_next_actions: vec!["secret recommendation prose".to_string()],
    };
    result.checks.push(vllm_doctor::core::models::RuleResult {
        id: "fake".into(),
        name: "Fake".into(),
        title: "secret finding title".into(),
        severity: vllm_doctor::core::models::Severity::Warning,
        finding: Some(vllm_doctor::core::models::Finding {
            severity: vllm_doctor::core::models::Severity::Warning,
            confidence: vllm_doctor::core::models::Confidence::High,
            title: "secret finding title".into(),
            signals: vec!["secret signal".into()],
            evidence: vec![vllm_doctor::core::models::EvidenceItem::text(
                "secret evidence",
            )],
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

#[test]
fn published_contract_deserializes_and_validates() {
    let json = include_str!("fixtures/contracts/observation-v1.json");
    let observation: ObservationV1 = serde_json::from_str(json).unwrap();
    validate_observation(&observation).unwrap();
}

#[test]
fn published_contract_defaults_legacy_source_to_prometheus() {
    let mut value = observation_value();
    value.as_object_mut().unwrap().remove("metrics_source");
    let observation = observation_from_value(value);
    assert_eq!(observation.metrics_source, MetricsSourceV1::Prometheus);
}

#[test]
fn published_contract_rejects_unknown_enum_values() {
    let json = include_str!("fixtures/contracts/observation-v1.json");
    let invalid = json.replace("\"engine\": \"vllm\"", "\"engine\": \"sglang\"");
    assert!(serde_json::from_str::<ObservationV1>(&invalid).is_err());
}

#[test]
fn published_contract_rejects_unsupported_schema_versions() {
    let json = include_str!("fixtures/contracts/observation-v1.json");
    let invalid = json.replace("\"schema_version\": 1", "\"schema_version\": 2");
    let observation: ObservationV1 = serde_json::from_str(&invalid).unwrap();
    assert_eq!(
        validate_observation(&observation),
        Err(ObservationValidationError::UnsupportedSchemaVersion)
    );
}

fn observation_value() -> Value {
    serde_json::from_str(include_str!("fixtures/contracts/observation-v1.json")).unwrap()
}

fn observation_from_value(value: Value) -> ObservationV1 {
    serde_json::from_value(value).unwrap()
}

#[tokio::test]
async fn every_registered_spec_round_trips() {
    let mut raw = std::collections::HashMap::new();
    for spec in all_specs() {
        for probe in spec.probe_names() {
            raw.insert(probe, MetricSeries::scalar(2.0));
        }
    }
    let snapshot = MetricSeriesSnapshot::from_raw(raw);
    let observation =
        build_batch_from_snapshot_with_source(snapshot.clone(), "prod", MetricsSource::Prometheus)
            .await;
    let reconstructed = reconstruct_snapshot(&observation).unwrap();

    for spec in all_specs() {
        assert_eq!(
            spec.extract(&reconstructed),
            spec.extract(&snapshot),
            "{} did not round trip",
            spec.observation_spec().id
        );
    }
}

async fn round_trip_diagnosis(
    snapshot: MetricSeriesSnapshot,
) -> (
    vllm_doctor::core::models::DiagnosisResult,
    vllm_doctor::core::models::DiagnosisResult,
) {
    let config = Config::default();
    let registry = build_registry(&config);
    let target = TargetMetadata {
        id: Some("parity-target".to_string()),
        environment: Some("production".to_string()),
        ..Default::default()
    };
    let local = diagnose(
        &StubProvider(snapshot),
        &registry,
        "5m",
        Some("model"),
        &target,
        &config,
    )
    .await
    .unwrap();
    let observation = build_observation(&local, &fixed_ctx(), 300).unwrap();
    let reconstructed_snapshot = reconstruct_snapshot(&observation).unwrap();
    let reconstructed = diagnose_snapshot(
        reconstructed_snapshot,
        diagnosis_context(&observation),
        &registry,
        &config,
    );
    assert_eq!(reconstructed.context.since, "300s");
    assert_eq!(reconstructed.context.target, target);
    assert_eq!(reconstructed.context.model_name.as_deref(), Some("model"));
    (local, reconstructed)
}

fn assert_diagnosis_parity(
    local: &vllm_doctor::core::models::DiagnosisResult,
    reconstructed: &vllm_doctor::core::models::DiagnosisResult,
) {
    assert_eq!(reconstructed.checks, local.checks);
    assert_eq!(reconstructed.assessment, local.assessment);
    assert_eq!(reconstructed.health(), local.health());
}

#[tokio::test]
async fn healthy_and_pressured_diagnoses_survive_round_trip() {
    let (healthy_local, healthy_reconstructed) =
        round_trip_diagnosis(MetricSeriesSnapshot::default()).await;
    assert_diagnosis_parity(&healthy_local, &healthy_reconstructed);

    let pressured = MetricSeriesSnapshot {
        num_requests_waiting: MetricSeries::scalar(8.0),
        num_requests_running: MetricSeries::scalar(60.0),
        kv_cache_usage_perc: MetricSeries::scalar(0.95),
        request_success_total: MetricSeries::scalar(1000.0),
        request_error_total: MetricSeries::scalar(80.0),
        request_abort_total: MetricSeries::scalar(30.0),
        ..Default::default()
    };
    let (pressured_local, pressured_reconstructed) = round_trip_diagnosis(pressured).await;
    assert_diagnosis_parity(&pressured_local, &pressured_reconstructed);
}

#[tokio::test]
async fn local_diagnosis_is_not_trusted_during_reconstruction() {
    let observation = build_batch_from_snapshot(golden_snapshot(), "prod").await;
    let baseline = reconstruct_snapshot(&observation).unwrap();
    let mut value = serde_json::to_value(&observation).unwrap();
    value["local_diagnosis"]["health"] = Value::String("critical".to_string());
    value["local_diagnosis"]["likely_bottleneck"] = Value::String("error_issue".to_string());
    value["local_diagnosis"]["confidence"] = Value::String("low".to_string());
    value["local_diagnosis"]["firing_rule_ids"] =
        serde_json::json!(["invented_rule", "another_rule"]);
    let changed = observation_from_value(value);

    assert_eq!(reconstruct_snapshot(&changed).unwrap(), baseline);
}

#[tokio::test]
async fn missing_measurements_remain_unknown() {
    let observation = build_batch_from_snapshot(MetricSeriesSnapshot::default(), "prod").await;
    let reconstructed = reconstruct_snapshot(&observation).unwrap();
    for spec in all_specs() {
        assert_eq!(spec.extract(&reconstructed), None);
    }
}

#[tokio::test]
async fn replica_imbalance_and_authoritative_aggregate_survive_round_trip() {
    let snapshot = MetricSeriesSnapshot {
        num_requests_running: MetricSeries {
            samples: vec![
                sample(20.0, &[("pod", "a"), ("model_name", "model")]),
                sample(2.0, &[("pod", "b"), ("model_name", "model")]),
            ],
            aggregate_by: Aggregate::Sum,
        },
        num_requests_waiting: MetricSeries {
            samples: vec![
                sample(4.0, &[("pod", "a"), ("model_name", "model")]),
                sample(0.0, &[("pod", "b"), ("model_name", "model")]),
            ],
            aggregate_by: Aggregate::Sum,
        },
        kv_cache_usage_perc: MetricSeries {
            samples: vec![
                sample(0.95, &[("pod", "a"), ("model_name", "model")]),
                sample(0.4, &[("pod", "b"), ("model_name", "model")]),
            ],
            aggregate_by: Aggregate::Max,
        },
        ..Default::default()
    };
    let (local, reconstructed) = round_trip_diagnosis(snapshot).await;
    assert_diagnosis_parity(&local, &reconstructed);
    assert!(
        reconstructed
            .checks
            .iter()
            .any(|check| check.id == "replica_imbalance" && check.finding.is_some())
    );
    assert!(
        reconstructed
            .checks
            .iter()
            .filter_map(|check| check.finding.as_ref())
            .flat_map(|finding| &finding.evidence)
            .any(|evidence| matches!(
                evidence,
                vllm_doctor::core::models::EvidenceItem::ReplicaDistribution {
                    model: Some(model),
                    ..
                } if model == "model"
            ))
    );
    assert_eq!(
        reconstructed.metric_series.num_requests_running.value(),
        Some(22.0)
    );
    assert_eq!(
        reconstructed
            .metric_series
            .num_requests_running
            .samples
            .len(),
        3,
        "one aggregate and two replicas should be retained without summing all three"
    );
}

#[test]
fn inbound_validation_rejects_unknown_ids_and_metadata_mismatches() {
    let mut unknown = observation_value();
    unknown["observations"][0]["id"] = Value::String("unknown_metric".to_string());
    assert!(matches!(
        validate_observation(&observation_from_value(unknown)),
        Err(ObservationValidationError::UnknownMeasurement(_))
    ));

    for (field, replacement) in [("unit", "seconds"), ("kind", "quantile"), ("rollup", "max")] {
        let mut value = observation_value();
        value["observations"][0][field] = Value::String(replacement.to_string());
        assert!(matches!(
            validate_observation(&observation_from_value(value)),
            Err(ObservationValidationError::MetadataMismatch { .. })
        ));
    }

    let mut quantile = observation_value();
    quantile["observations"][0]["quantile"] = serde_json::json!(0.95);
    assert!(matches!(
        validate_observation(&observation_from_value(quantile)),
        Err(ObservationValidationError::MetadataMismatch { .. })
    ));
}

#[test]
fn inbound_validation_rejects_duplicates_conflicts_and_dimensions() {
    let mut duplicate_aggregate = observation_value();
    let first = duplicate_aggregate["observations"][0].clone();
    duplicate_aggregate["observations"]
        .as_array_mut()
        .unwrap()
        .push(first);
    assert!(matches!(
        validate_observation(&observation_from_value(duplicate_aggregate)),
        Err(ObservationValidationError::DuplicateAggregate(_))
    ));

    let mut duplicate_replica = observation_value();
    let replica = duplicate_replica["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|measurement| !measurement["dimensions"].is_null())
        .unwrap()
        .clone();
    duplicate_replica["observations"]
        .as_array_mut()
        .unwrap()
        .push(replica);
    assert!(matches!(
        validate_observation(&observation_from_value(duplicate_replica)),
        Err(ObservationValidationError::DuplicateReplica { .. })
    ));

    let mut conflict = observation_value();
    let measured_id = conflict["observations"][0]["id"].clone();
    conflict["availability"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": measured_id,
            "status": "not_collected"
        }));
    assert!(matches!(
        validate_observation(&observation_from_value(conflict)),
        Err(ObservationValidationError::ConflictingAvailability(_))
    ));

    let mut dimensions = observation_value();
    dimensions["observations"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "id": "requests_succeeded",
            "unit": "count",
            "kind": "window_delta",
            "rollup": "sum",
            "dimensions": { "replica": "replica-1" },
            "value": 1.0
        }));
    assert!(matches!(
        validate_observation(&observation_from_value(dimensions)),
        Err(ObservationValidationError::InvalidDimensions(_))
    ));
}

#[test]
fn inbound_wire_types_reject_unknown_fields() {
    let mut root = observation_value();
    root["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ObservationV1>(root).is_err());

    let mut nested = observation_value();
    nested["observations"][0]["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ObservationV1>(nested).is_err());
}
