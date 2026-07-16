//! Root-cause assessment: reduces the fired findings and metrics of a diagnosis
//! run into a single "Likely bottleneck" interpretation.
//!
//! This is a downstream, read-only reducer over [`DiagnosisResult`]. It consumes
//! rule *results* (which rules fired, and the evidence they already recorded) —
//! it never re-runs rule thresholds, so it cannot drift from the rules layer.
use crate::models::{
    Assessment, BottleneckKind, Confidence, DiagnosisResult, EvidenceItem, Health, RuleResult,
};

/// Interpret a completed diagnosis into its most likely bottleneck.
///
/// An idle classification requires explicitly observed zero traffic; absent
/// metrics remain unknown. Fired rules are evaluated in domain-priority order,
/// with failures and saturation ahead of latency shape. High TTFT maps to
/// queueing when requests are waiting and to long prefill otherwise.
pub fn assess(result: &DiagnosisResult) -> Assessment {
    let metrics = &result.metric_series;

    if result.health() == Health::Ok {
        let running = metrics.num_requests_running.value();
        let waiting = metrics.num_requests_waiting.value();
        if running == Some(0.0) && waiting == Some(0.0) {
            return build(BottleneckKind::Idle, Confidence::High, &[], &[]);
        }
        return build(BottleneckKind::NoClearBottleneck, Confidence::Low, &[], &[]);
    }

    let fired: Vec<&RuleResult> = result
        .checks
        .iter()
        .filter(|c| c.finding.is_some())
        .collect();
    let has = |id: &str| fired.iter().any(|c| c.id == id);

    let (kind, support): (BottleneckKind, &[&str]) = if has("error_rate") {
        (BottleneckKind::ErrorIssue, &["error_rate"])
    } else if has("replica_imbalance") && !has("kv_cache_pressure") && !has("queue_pressure") {
        (BottleneckKind::ReplicaImbalance, &["replica_imbalance"])
    } else if has("kv_cache_pressure") {
        (
            BottleneckKind::KvCacheSaturation,
            &["kv_cache_pressure", "queue_pressure", "ttft_bottleneck"],
        )
    } else if has("queue_pressure") || has("queue_latency") {
        (
            BottleneckKind::QueueSaturation,
            &["queue_pressure", "queue_latency"],
        )
    } else if has("ttft_bottleneck") && !has("tpot_bottleneck") {
        let waiting = metrics.num_requests_waiting.value().unwrap_or(0.0);
        if waiting > 0.0 {
            (
                BottleneckKind::QueueSaturation,
                &["ttft_bottleneck", "queue_pressure"],
            )
        } else {
            (BottleneckKind::LongPrefill, &["ttft_bottleneck"])
        }
    } else if has("tpot_bottleneck") {
        (
            BottleneckKind::DecodeBottleneck,
            &["tpot_bottleneck", "low_throughput"],
        )
    } else {
        return build(BottleneckKind::NoClearBottleneck, Confidence::Low, &[], &[]);
    };

    let confidence = confidence_for(&fired, support);
    build(kind, confidence, &fired, support)
}

/// Assemble the summary: real evidence pulled from the supporting findings,
/// plus the templated interpretation and next actions for the category.
fn build(
    kind: BottleneckKind,
    confidence: Confidence,
    fired: &[&RuleResult],
    support: &[&str],
) -> Assessment {
    let (interpretation, actions) = narrative(kind);
    Assessment {
        likely_bottleneck: kind,
        confidence,
        evidence: evidence_from(fired, support, kind),
        interpretation: interpretation.to_string(),
        recommended_next_actions: actions.iter().map(|s| (*s).to_string()).collect(),
    }
}

/// Evidence is drawn from the findings that actually fired for this run, so the
/// bullets carry the run's real numbers rather than restating thresholds.
fn evidence_from(
    fired: &[&RuleResult],
    support: &[&str],
    kind: BottleneckKind,
) -> Vec<EvidenceItem> {
    if kind == BottleneckKind::Idle {
        return vec![EvidenceItem::text(
            "No running or waiting requests were observed",
        )];
    }
    let mut items = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for id in support {
        let finding = fired
            .iter()
            .find(|c| c.id.as_str() == *id)
            .and_then(|c| c.finding.as_ref());
        let Some(f) = finding else { continue };
        for item in &f.evidence {
            let key = item.metric_key();
            if seen.insert(key) {
                items.push(item.clone());
            }
        }
    }
    items
}

/// Confidence grows with corroboration: the primary finding's own confidence,
/// bumped up when independent supporting findings also fired.
fn confidence_for(fired: &[&RuleResult], support: &[&str]) -> Confidence {
    let primary = support.first().copied().unwrap_or_default();
    let base = fired
        .iter()
        .find(|c| c.id.as_str() == primary)
        .and_then(|c| c.finding.as_ref())
        .map(|f| f.confidence)
        .unwrap_or(Confidence::Low);
    let corroborating = support
        .iter()
        .filter(|id| fired.iter().any(|c| c.id.as_str() == **id))
        .count();
    if corroborating >= 3 {
        Confidence::High
    } else if corroborating >= 2 {
        match base {
            Confidence::High => Confidence::High,
            _ => Confidence::Medium,
        }
    } else {
        base
    }
}

/// Static, per-category explanation and safe next steps. Intentionally
/// conservative — no speculation beyond what the findings support.
fn narrative(kind: BottleneckKind) -> (&'static str, &'static [&'static str]) {
    match kind {
        BottleneckKind::QueueSaturation => (
            "Requests are arriving faster than the server can process them; the queue is \
             growing and adding latency to every incoming request.",
            &[
                "Add replicas or raise concurrency limits",
                "Review autoscaling thresholds",
                "Shed or slow the incoming request rate if this is unexpected",
            ],
        ),
        BottleneckKind::KvCacheSaturation => (
            "Requests are likely waiting because the server has limited KV cache headroom, \
             often caused by high concurrency or long-context requests.",
            &[
                "Check max_num_seqs and max_num_batched_tokens",
                "Route long-context traffic separately",
                "Add capacity or reduce concurrency if this happens during expected traffic",
            ],
        ),
        BottleneckKind::LongPrefill => (
            "High time to first token with normal decode latency and no queue suggests long \
             input prompts are dominating prefill time.",
            &[
                "Enable or tune chunked prefill (--enable-chunked-prefill)",
                "Reduce max prompt length or filter very long requests",
                "Separate long-context traffic onto dedicated instances",
            ],
        ),
        BottleneckKind::DecodeBottleneck => (
            "Each output token is taking too long to generate, pointing at GPU decode \
             saturation or memory-bandwidth pressure rather than queueing.",
            &[
                "Check GPU utilization and memory bandwidth",
                "Reduce batch size or parallelism overhead",
                "Consider a faster GPU or more replicas",
            ],
        ),
        BottleneckKind::ReplicaImbalance => (
            "Load is unevenly distributed across replicas, overloading some while others sit \
             idle.",
            &[
                "Check load-balancer routing and session-affinity settings",
                "Verify readiness probes — an unready replica receives no traffic",
                "Compare per-replica latency and restart any unhealthy replica",
            ],
        ),
        BottleneckKind::ErrorIssue => (
            "The server is returning errors or aborting requests, which may indicate a crash, \
             OOM, or invalid request handling.",
            &[
                "Check the vLLM server logs for crash or OOM messages",
                "Verify model configuration and input validation",
                "Watch the error rate after restarting the server",
            ],
        ),
        BottleneckKind::Idle => (
            "The server appears idle with no active traffic, so throughput and latency \
             warnings are suppressed.",
            &[
                "Generate representative load and re-run the diagnosis",
                "Confirm the target endpoint is actually receiving traffic",
            ],
        ),
        BottleneckKind::NoClearBottleneck => {
            ("No single bottleneck clearly dominates this run.", &[])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::{
        Confidence, DiagnosisContext, DiagnosisResult, EvidenceItem, Finding, RuleResult, Severity,
    };

    fn snapshot(running: f64, waiting: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(running)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(waiting)]),
            ..Default::default()
        }
    }

    fn check(id: &str, severity: Severity, confidence: Confidence) -> RuleResult {
        RuleResult {
            id: id.into(),
            name: id.into(),
            title: id.into(),
            severity,
            finding: Some(Finding {
                severity,
                confidence,
                title: id.into(),
                signals: vec![],
                evidence: vec![EvidenceItem::text(format!("{id} evidence"))],
                likely_causes: vec![],
                recommendations: vec![],
                related_metrics: vec![],
            }),
        }
    }

    fn healthy_check(id: &str) -> RuleResult {
        RuleResult {
            id: id.into(),
            name: id.into(),
            title: id.into(),
            severity: Severity::Info,
            finding: None,
        }
    }

    fn result(checks: Vec<RuleResult>, metrics: MetricSeriesSnapshot) -> DiagnosisResult {
        DiagnosisResult::new(DiagnosisContext::new("5m"), metrics, checks)
    }

    #[test]
    fn idle_server() {
        let s = assess(&result(
            vec![healthy_check("low_throughput")],
            snapshot(0.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::Idle);
        assert_eq!(s.confidence, Confidence::High);
        assert!(!s.evidence.is_empty());
    }

    #[test]
    fn healthy_under_load_is_not_idle() {
        let s = assess(&result(
            vec![healthy_check("queue_pressure")],
            snapshot(10.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::NoClearBottleneck);
    }

    #[test]
    fn absent_traffic_metrics_are_not_idle() {
        let s = assess(&result(
            vec![healthy_check("tpot_bottleneck")],
            MetricSeriesSnapshot::default(),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::NoClearBottleneck);
    }

    #[test]
    fn kv_cache_saturation() {
        let s = assess(&result(
            vec![
                check("kv_cache_pressure", Severity::Critical, Confidence::High),
                check("ttft_bottleneck", Severity::Warning, Confidence::High),
                check("queue_pressure", Severity::Warning, Confidence::Low),
            ],
            snapshot(12.0, 7.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::KvCacheSaturation);
        assert_eq!(s.confidence, Confidence::High);
        assert!(
            s.evidence
                .iter()
                .any(|e| e.metric_key().contains("kv_cache_pressure"))
        );
    }

    #[test]
    fn queue_saturation() {
        let s = assess(&result(
            vec![
                check("queue_pressure", Severity::Warning, Confidence::High),
                check("queue_latency", Severity::Warning, Confidence::Medium),
            ],
            snapshot(5.0, 10.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::QueueSaturation);
        assert_eq!(s.confidence, Confidence::High);
    }

    #[test]
    fn long_prefill_when_ttft_high_and_no_queue() {
        let s = assess(&result(
            vec![check(
                "ttft_bottleneck",
                Severity::Warning,
                Confidence::High,
            )],
            snapshot(3.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::LongPrefill);
    }

    #[test]
    fn ttft_with_queue_is_queueing_not_prefill() {
        let s = assess(&result(
            vec![check(
                "ttft_bottleneck",
                Severity::Warning,
                Confidence::High,
            )],
            snapshot(3.0, 8.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::QueueSaturation);
    }

    #[test]
    fn decode_bottleneck() {
        let s = assess(&result(
            vec![check(
                "tpot_bottleneck",
                Severity::Warning,
                Confidence::High,
            )],
            snapshot(2.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::DecodeBottleneck);
    }

    #[test]
    fn replica_imbalance() {
        let s = assess(&result(
            vec![check(
                "replica_imbalance",
                Severity::Warning,
                Confidence::High,
            )],
            snapshot(10.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::ReplicaImbalance);
    }

    #[test]
    fn error_issue() {
        let s = assess(&result(
            vec![check("error_rate", Severity::Critical, Confidence::High)],
            snapshot(5.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::ErrorIssue);
    }

    #[test]
    fn no_clear_bottleneck_on_weak_lone_signal() {
        let s = assess(&result(
            vec![check("low_throughput", Severity::Warning, Confidence::Low)],
            snapshot(1.0, 0.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::NoClearBottleneck);
        assert_eq!(s.confidence, Confidence::Low);
    }

    #[test]
    fn errors_take_priority_over_saturation() {
        let s = assess(&result(
            vec![
                check("error_rate", Severity::Critical, Confidence::High),
                check("kv_cache_pressure", Severity::Critical, Confidence::High),
            ],
            snapshot(12.0, 7.0),
        ));
        assert_eq!(s.likely_bottleneck, BottleneckKind::ErrorIssue);
    }
}
