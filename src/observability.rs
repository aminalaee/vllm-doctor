//! Process-level observation state and Prometheus exposition for continuous
//! diagnosis. HTTP transport and process lifecycle remain in the CLI layer.
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};

use crate::models::{DiagnosisResult, Health, InferenceEngine, Severity, TargetMetadata};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveFinding {
    rule: String,
    severity: Severity,
}

#[derive(Debug, Clone, Default)]
struct ObservationSnapshot {
    last_health: Option<Health>,
    last_success: Option<DateTime<Utc>>,
    latest_attempt_succeeded: bool,
    active_findings: Vec<ActiveFinding>,
}

pub struct AgentState {
    target_id: Option<String>,
    engine: InferenceEngine,
    snapshot: RwLock<ObservationSnapshot>,
    collection_errors: AtomicU64,
}

impl AgentState {
    pub fn new(target: TargetMetadata) -> Self {
        Self {
            target_id: target.id,
            engine: target.engine,
            snapshot: RwLock::new(ObservationSnapshot::default()),
            collection_errors: AtomicU64::new(0),
        }
    }

    pub fn record_success(&self, result: &DiagnosisResult, at: DateTime<Utc>) {
        let active_findings = result
            .checks
            .iter()
            .filter_map(|check| {
                check.finding.as_ref().map(|finding| ActiveFinding {
                    rule: check.id.clone(),
                    severity: finding.severity,
                })
            })
            .collect();
        let mut snapshot = write_lock(&self.snapshot);
        snapshot.last_health = Some(result.health());
        snapshot.last_success = Some(at);
        snapshot.latest_attempt_succeeded = true;
        snapshot.active_findings = active_findings;
    }

    pub fn record_error(&self) {
        self.collection_errors.fetch_add(1, Ordering::Relaxed);
        write_lock(&self.snapshot).latest_attempt_succeeded = false;
    }

    pub fn is_ready(&self) -> bool {
        read_lock(&self.snapshot).latest_attempt_succeeded
    }
}

pub fn render_metrics(state: &AgentState) -> String {
    let snapshot = read_lock(&state.snapshot).clone();
    let target = escape_label_value(state.target_id.as_deref().unwrap_or("unconfigured"));
    let engine = escape_label_value(&state.engine.to_string());
    let labels = format!("target=\"{target}\",engine=\"{engine}\"");
    let ready = u8::from(snapshot.latest_attempt_succeeded);
    let health = snapshot.last_health.map_or(-1, health_value);
    let last_success = snapshot
        .last_success
        .map_or(0, |timestamp| timestamp.timestamp());
    let errors = state.collection_errors.load(Ordering::Relaxed);

    let mut output = String::new();
    metric_header(
        &mut output,
        "ready",
        "Whether the latest collection attempt succeeded",
        "gauge",
    );
    writeln!(output, "vllm_doctor_ready{{{labels}}} {ready}").unwrap();
    metric_header(
        &mut output,
        "target_health",
        "Last known target health (-1 unknown, 0 healthy, 1 info, 2 warning, 3 critical)",
        "gauge",
    );
    writeln!(output, "vllm_doctor_target_health{{{labels}}} {health}").unwrap();
    metric_header(
        &mut output,
        "last_success_timestamp_seconds",
        "Unix timestamp of the last successful diagnosis",
        "gauge",
    );
    writeln!(
        output,
        "vllm_doctor_last_success_timestamp_seconds{{{labels}}} {last_success}"
    )
    .unwrap();
    metric_header(
        &mut output,
        "collection_errors_total",
        "Cumulative provider setup and collection errors",
        "counter",
    );
    writeln!(
        output,
        "vllm_doctor_collection_errors_total{{{labels}}} {errors}"
    )
    .unwrap();
    metric_header(
        &mut output,
        "finding",
        "Findings from the last successful diagnosis",
        "gauge",
    );
    for finding in snapshot.active_findings {
        let rule = escape_label_value(&finding.rule);
        let severity = escape_label_value(&finding.severity.to_string());
        writeln!(
            output,
            "vllm_doctor_finding{{{labels},rule=\"{rule}\",severity=\"{severity}\"}} 1"
        )
        .unwrap();
    }
    output
}

fn metric_header(output: &mut String, name: &str, help: &str, metric_type: &str) {
    writeln!(output, "# HELP vllm_doctor_{name} {help}").unwrap();
    writeln!(output, "# TYPE vllm_doctor_{name} {metric_type}").unwrap();
}

fn health_value(health: Health) -> i8 {
    match health {
        Health::Ok => 0,
        Health::Info => 1,
        Health::Warning => 2,
        Health::Critical => 3,
    }
}

fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('"', "\\\"")
}

fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::TimeZone;

    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::models::{Confidence, DiagnosisContext, Finding, RuleResult};

    fn result(findings: &[(&str, Severity)]) -> DiagnosisResult {
        let checks = findings
            .iter()
            .map(|(id, severity)| RuleResult {
                id: (*id).to_string(),
                name: (*id).to_string(),
                title: (*id).to_string(),
                severity: *severity,
                finding: Some(Finding {
                    severity: *severity,
                    confidence: Confidence::High,
                    title: (*id).to_string(),
                    signals: vec![],
                    evidence: vec![],
                    likely_causes: vec![],
                    recommendations: vec![],
                    related_metrics: vec![],
                }),
            })
            .collect();
        DiagnosisResult::new(
            DiagnosisContext::new("5m"),
            MetricSeriesSnapshot::default(),
            checks,
        )
    }

    #[test]
    fn initial_state_is_unknown_and_unready() {
        let state = AgentState::new(TargetMetadata::default());
        let metrics = render_metrics(&state);
        assert!(!state.is_ready());
        assert!(metrics.contains("vllm_doctor_ready{target=\"unconfigured\",engine=\"vllm\"} 0"));
        assert!(
            metrics
                .contains("vllm_doctor_target_health{target=\"unconfigured\",engine=\"vllm\"} -1")
        );
        assert!(metrics.ends_with('\n'));
    }

    #[test]
    fn success_error_and_recovery_preserve_last_diagnosis() {
        let state = AgentState::new(TargetMetadata::default());
        let first = result(&[("queue_pressure", Severity::Warning)]);
        state.record_success(&first, Utc.timestamp_opt(100, 0).unwrap());
        assert!(state.is_ready());
        state.record_error();
        state.record_error();
        assert!(!state.is_ready());
        let metrics = render_metrics(&state);
        assert!(
            metrics
                .contains("vllm_doctor_target_health{target=\"unconfigured\",engine=\"vllm\"} 2")
        );
        assert!(metrics.contains(
            "vllm_doctor_collection_errors_total{target=\"unconfigured\",engine=\"vllm\"} 2"
        ));
        assert!(metrics.contains("rule=\"queue_pressure\",severity=\"warning\""));

        state.record_success(&result(&[]), Utc.timestamp_opt(200, 0).unwrap());
        assert!(state.is_ready());
        let metrics = render_metrics(&state);
        assert!(metrics.contains("vllm_doctor_last_success_timestamp_seconds{target=\"unconfigured\",engine=\"vllm\"} 200"));
        assert!(!metrics.contains("vllm_doctor_finding{"));
    }

    #[test]
    fn label_values_are_escaped() {
        let state = AgentState::new(TargetMetadata {
            id: Some("quoted\"\\\nvalue".to_string()),
            ..TargetMetadata::default()
        });
        state.record_success(
            &result(&[("rule\"\\\nname", Severity::Critical)]),
            Utc.timestamp_opt(100, 0).unwrap(),
        );
        let metrics = render_metrics(&state);
        assert!(metrics.contains("target=\"quoted\\\"\\\\\\nvalue\""));
        assert!(metrics.contains("rule=\"rule\\\"\\\\\\nname\""));
    }

    #[test]
    fn prometheus_contract_is_exact() {
        let state = AgentState::new(TargetMetadata {
            id: Some("production".to_string()),
            ..TargetMetadata::default()
        });
        state.record_success(
            &result(&[("queue_pressure", Severity::Warning)]),
            Utc.timestamp_opt(100, 0).unwrap(),
        );

        assert_eq!(
            render_metrics(&state),
            concat!(
                "# HELP vllm_doctor_ready Whether the latest collection attempt succeeded\n",
                "# TYPE vllm_doctor_ready gauge\n",
                "vllm_doctor_ready{target=\"production\",engine=\"vllm\"} 1\n",
                "# HELP vllm_doctor_target_health Last known target health (-1 unknown, 0 healthy, 1 info, 2 warning, 3 critical)\n",
                "# TYPE vllm_doctor_target_health gauge\n",
                "vllm_doctor_target_health{target=\"production\",engine=\"vllm\"} 2\n",
                "# HELP vllm_doctor_last_success_timestamp_seconds Unix timestamp of the last successful diagnosis\n",
                "# TYPE vllm_doctor_last_success_timestamp_seconds gauge\n",
                "vllm_doctor_last_success_timestamp_seconds{target=\"production\",engine=\"vllm\"} 100\n",
                "# HELP vllm_doctor_collection_errors_total Cumulative provider setup and collection errors\n",
                "# TYPE vllm_doctor_collection_errors_total counter\n",
                "vllm_doctor_collection_errors_total{target=\"production\",engine=\"vllm\"} 0\n",
                "# HELP vllm_doctor_finding Findings from the last successful diagnosis\n",
                "# TYPE vllm_doctor_finding gauge\n",
                "vllm_doctor_finding{target=\"production\",engine=\"vllm\",rule=\"queue_pressure\",severity=\"warning\"} 1\n",
            )
        );
    }

    #[test]
    fn poisoned_snapshot_lock_is_recovered() {
        let state = Arc::new(AgentState::new(TargetMetadata::default()));
        let poisoned = Arc::clone(&state);
        let _ = std::thread::spawn(move || {
            let _guard = poisoned.snapshot.write().unwrap();
            panic!("poison observation snapshot");
        })
        .join();

        assert!(!state.is_ready());
        assert!(render_metrics(&state).contains("vllm_doctor_ready"));
    }
}
