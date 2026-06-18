//! JSON report renderer.
//!
//! Schema version "1": a `metadata` block, the health rollup, advisory `notices`,
//! per-rule `checks` (each with an embedded finding or null), and an optional
//! verbose `metrics` block.
use crate::metrics::{MetricSpec, all_specs, detect_replica_label};
use crate::models::{Finding, RuleResult};
use crate::reports::Report;
use crate::reports::notices::resolve_notices;
use serde_json::{Map, Value, json};

const SCHEMA_VERSION: &str = "1";

/// Render a report as JSON.
pub fn render(report: &Report, verbose: bool) -> Value {
    let context = &report.diagnosis.context;
    let checks: Vec<Value> = report.checks().iter().map(check_json).collect();

    let mut payload = json!({
        "schema_version": SCHEMA_VERSION,
        "metadata": {
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "target": {
                "model_name": context.model_name,
                "since": context.since,
                "client_mode": context.client_mode.to_string(),
            },
        },
        "health": report.health().to_string(),
        "notices": resolve_notices(&report.diagnosis),
        "checks": checks,
    });

    if verbose {
        payload["metrics"] = metrics_json(report);
    }

    payload
}

fn check_json(check: &RuleResult) -> Value {
    json!({
        "id": check.id,
        "name": check.name,
        "finding": check.finding.as_ref().map(finding_json),
    })
}

fn finding_json(finding: &Finding) -> Value {
    // `signals` is intentionally omitted from JSON — it is an internal detail.
    json!({
        "severity": finding.severity.to_string(),
        "confidence": finding.confidence.to_string(),
        "title": finding.title,
        "summary": finding.summary,
        "evidence": finding.evidence,
        "likely_causes": finding.likely_causes,
        "recommendations": finding.recommendations,
        "related_metrics": finding.related_metrics,
    })
}

fn metrics_json(report: &Report) -> Value {
    let series = report.metric_series();
    let label = detect_replica_label(series);
    let mut values = Map::new();
    for spec in all_specs() {
        let spec: &dyn MetricSpec = spec.as_ref();
        let mut entry = Map::new();
        entry.insert(
            "value".to_string(),
            spec.extract(series).map_or(Value::Null, |v| json!(v)),
        );
        if let Some(label) = label {
            if let Some(metric_series) = spec.series(series) {
                let breakdown = metric_series.by(label);
                if breakdown.len() > 1 {
                    let by: Map<String, Value> = breakdown
                        .into_iter()
                        .map(|(k, v)| (k, v.map_or(Value::Null, |x| json!(x))))
                        .collect();
                    entry.insert("by".to_string(), json!({ label: by }));
                }
            }
        }
        values.insert(spec.output().to_string(), Value::Object(entry));
    }
    Value::Object(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::{
        Confidence, DiagnosisContext, DiagnosisResult, Finding, RuleResult, Severity,
    };

    fn finding() -> Finding {
        Finding {
            severity: Severity::Warning,
            confidence: Confidence::Medium,
            title: "Queue pressure".into(),
            summary: "5 requests waiting".into(),
            signals: vec!["num_requests_waiting".into()],
            evidence: vec!["Waiting requests: 5".into()],
            likely_causes: vec!["Insufficient capacity".into()],
            recommendations: vec!["Add replicas".into()],
            related_metrics: vec!["vllm:num_requests_waiting".into()],
        }
    }

    fn report(metric_series: MetricSeriesSnapshot) -> Report {
        Report::new(DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            metric_series,
            checks: vec![RuleResult {
                id: "queue_pressure",
                name: "Queue Pressure",
                title: "Queue pressure",
                severity: Severity::Warning,
                finding: Some(finding()),
            }],
        })
    }

    #[test]
    fn schema_and_metadata_present() {
        let json = render(&report(MetricSeriesSnapshot::default()), false);
        assert_eq!(json["schema_version"], "1");
        assert_eq!(json["health"], "warning");
        assert_eq!(json["metadata"]["target"]["since"], "5m");
        assert_eq!(json["metadata"]["target"]["client_mode"], "prometheus");
        assert!(json["metadata"]["generated_at"].is_string());
        assert!(json["notices"].as_array().unwrap().is_empty());
    }

    #[test]
    fn finding_embedded_in_check_and_omits_signals() {
        let json = render(&report(MetricSeriesSnapshot::default()), false);
        let check = &json["checks"][0];
        assert_eq!(check["id"], "queue_pressure");
        let finding = &check["finding"];
        assert_eq!(finding["title"], "Queue pressure");
        assert!(finding.get("signals").is_none());
    }

    #[test]
    fn health_ok_serializes_as_healthy() {
        let report = Report::new(DiagnosisResult {
            context: DiagnosisContext::new("5m"),
            metric_series: MetricSeriesSnapshot::default(),
            checks: vec![],
        });
        let json = render(&report, false);
        assert_eq!(json["health"], "healthy");
        assert_eq!(json["checks"][0], Value::Null);
    }

    #[test]
    fn metrics_only_when_verbose() {
        let report = report(MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
            ..Default::default()
        });
        assert!(render(&report, false).get("metrics").is_none());
        let verbose = render(&report, true);
        assert_eq!(verbose["metrics"]["num_requests_waiting"]["value"], 5.0);
    }

    #[test]
    fn metrics_include_replica_breakdown() {
        let report = report(MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![
                MetricSample::new(5.0).with_label("pod", "a"),
                MetricSample::new(1.0).with_label("pod", "b"),
            ]),
            ..Default::default()
        });
        let verbose = render(&report, true);
        let by = &verbose["metrics"]["num_requests_waiting"]["by"]["pod"];
        assert_eq!(by["a"], 5.0);
        assert_eq!(by["b"], 1.0);
    }
}
