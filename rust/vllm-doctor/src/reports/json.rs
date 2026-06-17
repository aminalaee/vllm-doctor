//! JSON report renderer.
use crate::metrics::all_specs;
use crate::models::{Finding, RuleResult};
use crate::reports::Report;
use crate::reports::format::format_value;
use serde_json::{Value, json};

/// Render a report as JSON.
pub fn render(report: &Report, verbose: bool) -> Value {
    let checks: Vec<Value> = report.checks().iter().map(check_json).collect();

    let findings: Vec<Value> = report
        .checks()
        .iter()
        .filter_map(|check| check.finding.as_ref().map(|f| finding_json(check, f)))
        .collect();

    let mut payload = json!({
        "schema_version": "1.0",
        "health": report.health().to_string(),
        "summary": {
            "total_checks": report.checks().len(),
            "fired_checks": findings.len()
        },
        "checks": checks,
        "findings": findings,
    });

    if verbose {
        if let Some(metrics) = metrics_json(report) {
            payload["metrics"] = metrics;
        }
    }

    payload
}

fn check_json(check: &RuleResult) -> Value {
    json!({
        "id": check.id,
        "name": check.name,
        "title": check.title,
        "severity": check.severity.to_string(),
        "fired": check.finding.is_some(),
    })
}

fn finding_json(check: &RuleResult, finding: &Finding) -> Value {
    json!({
        "id": check.id,
        "title": finding.title,
        "severity": finding.severity.to_string(),
        "confidence": finding.confidence.to_string(),
        "summary": finding.summary,
        "signals": finding.signals,
        "evidence": finding.evidence,
        "likely_causes": finding.likely_causes,
        "recommendations": finding.recommendations,
        "related_metrics": finding.related_metrics,
    })
}

fn metrics_json(report: &Report) -> Option<Value> {
    let mut values = serde_json::Map::new();
    for spec in all_specs() {
        let spec: &dyn crate::metrics::MetricSpec = spec.as_ref();
        if let Some(value) = spec.extract(report.metric_series()) {
            values.insert(
                spec.output().to_string(),
                json!({
                    "title": spec.display().title,
                    "value": value,
                    "formatted": format_value(value, spec.display().fmt.as_str()),
                }),
            );
        }
    }
    if values.is_empty() {
        return None;
    }
    Some(Value::Object(values))
}
