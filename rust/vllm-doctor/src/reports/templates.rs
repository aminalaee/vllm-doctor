//! Per-rule finding templates.
use crate::models::DiagnosisState;
use crate::signals::{Signal, SignalGraph};

/// Context provided to a template when formatting a finding.
pub struct TemplateContext<'a> {
    pub graph: &'a SignalGraph<'a>,
    pub state: &'a DiagnosisState,
}

/// Formats a rule's judgment state into human-readable summary and evidence.
pub trait FindingTemplate: Sync {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String;
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String>;
}

fn signal_and_value(ctx: &TemplateContext<'_>) -> Option<(Signal, f64)> {
    match ctx.state {
        DiagnosisState::Stressed(signal, value) | DiagnosisState::Saturated(signal, value) => {
            Some((*signal, *value))
        }
        _ => None,
    }
}

/// Generic fallback template used when a rule does not provide a custom one.
pub struct GenericTemplate;

impl FindingTemplate for GenericTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((signal, _)) = signal_and_value(ctx) {
            format!("{signal} is elevated")
        } else {
            "Check could not be evaluated".into()
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        if let Some((signal, value)) = signal_and_value(ctx) {
            vec![format!("{signal} = {value:.4}")]
        } else {
            vec![]
        }
    }
}

pub struct QueuePressureTemplate;

impl FindingTemplate for QueuePressureTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((_, waiting)) = signal_and_value(ctx) {
            format!("{waiting:.0} requests are waiting in the queue")
        } else {
            GenericTemplate.summary(ctx)
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let Some((_, waiting)) = signal_and_value(ctx) else {
            return GenericTemplate.evidence(ctx);
        };
        let mut lines = vec![format!("Waiting requests: {waiting:.0}")];
        if let Some(running) = ctx.graph.evaluate(Signal::NumRequestsRunning) {
            lines.push(format!("Running requests: {running:.0}"));
        }
        lines
    }
}

pub struct QueueLatencyTemplate;

impl FindingTemplate for QueueLatencyTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((_, queue_time)) = signal_and_value(ctx) {
            format!(
                "Requests are waiting {queue_time:.2}s (p95) in the queue before prefill begins"
            )
        } else {
            GenericTemplate.summary(ctx)
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let Some((_, queue_time)) = signal_and_value(ctx) else {
            return GenericTemplate.evidence(ctx);
        };
        let mut lines = vec![format!("Queue time p95: {queue_time:.3}s")];
        if let Some(waiting) = ctx.graph.evaluate(Signal::NumRequestsWaiting) {
            if waiting > 0.0 {
                lines.push(format!("Waiting requests: {waiting:.0}"));
            }
        }
        lines
    }
}

pub struct KvCachePressureTemplate;

impl FindingTemplate for KvCachePressureTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((_, usage)) = signal_and_value(ctx) {
            format!("GPU KV cache usage is at {}%", usage * 100.0)
        } else {
            GenericTemplate.summary(ctx)
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let Some((_, usage)) = signal_and_value(ctx) else {
            return GenericTemplate.evidence(ctx);
        };
        vec![format!("KV cache usage: {}%", usage * 100.0)]
    }
}

pub struct PreemptionPressureTemplate;

impl FindingTemplate for PreemptionPressureTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Requests are being preempted — the engine is evicting sequences to free KV cache".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(usage) = ctx.graph.evaluate(Signal::KvCacheUsagePerc) {
            lines.push(format!("KV cache usage: {}%", usage * 100.0));
        }
        // The firing state value carries the preemption count; fall back to the graph.
        let preemptions = signal_and_value(ctx)
            .map(|(_, count)| count)
            .or_else(|| ctx.graph.evaluate(Signal::NumPreemptionsTotal));
        if let Some(preemptions) = preemptions {
            lines.push(format!("Preemptions total: {preemptions:.0}"));
        }
        if lines.is_empty() {
            return GenericTemplate.evidence(ctx);
        }
        lines
    }
}

pub struct LowThroughputTemplate;

impl FindingTemplate for LowThroughputTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Token throughput is lower than expected for the current load".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let mut lines = vec![];
        if let Some(prompt) = ctx.graph.evaluate(Signal::PromptTokensPerSecond) {
            lines.push(format!("Prefill throughput: {prompt:.1} tok/s"));
        }
        if let Some(decode) = ctx.graph.evaluate(Signal::GenerationTokensPerSecond) {
            lines.push(format!("Decode throughput: {decode:.1} tok/s"));
        }
        if let Some(running) = ctx.graph.evaluate(Signal::NumRequestsRunning) {
            lines.push(format!("Running requests: {running:.0}"));
        }
        lines
    }
}

pub struct ErrorRateTemplate;

impl FindingTemplate for ErrorRateTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Server is returning errors or clients are aborting at an elevated rate".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let errors = ctx.graph.evaluate(Signal::RequestErrorTotal).unwrap_or(0.0);
        let aborts = ctx.graph.evaluate(Signal::RequestAbortTotal).unwrap_or(0.0);
        let success = ctx
            .graph
            .evaluate(Signal::RequestSuccessTotal)
            .unwrap_or(0.0);
        let total = errors + aborts + success;
        if total == 0.0 {
            return vec!["No request data available".into()];
        }
        let mut lines = vec![];
        let error_rate = errors / total;
        let abort_rate = aborts / total;
        if errors > 0.0 {
            lines.push(format!(
                "Error rate: {:.1}% ({errors:.0} errors out of {total:.0} requests)",
                error_rate * 100.0
            ));
        }
        if aborts > 0.0 {
            lines.push(format!(
                "Abort rate: {:.1}% ({aborts:.0} aborts out of {total:.0} requests)",
                abort_rate * 100.0
            ));
        }
        lines
    }
}

pub struct TtftBottleneckTemplate;

impl FindingTemplate for TtftBottleneckTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((_, ttft)) = signal_and_value(ctx) {
            format!("TTFT p95 is {ttft:.2}s — requests wait too long before the first token")
        } else {
            GenericTemplate.summary(ctx)
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let Some((_, ttft)) = signal_and_value(ctx) else {
            return GenericTemplate.evidence(ctx);
        };
        let mut lines = vec![format!("TTFT p95: {ttft:.3}s")];
        if let Some(tpot) = ctx.graph.evaluate(Signal::TpotP95Seconds) {
            lines.push(format!("TPOT p95: {tpot:.3}s"));
        }
        if let Some(waiting) = ctx.graph.evaluate(Signal::NumRequestsWaiting) {
            if waiting > 0.0 {
                lines.push(format!("Waiting requests: {waiting:.0}"));
            }
        }
        lines
    }
}

pub struct TpotBottleneckTemplate;

impl FindingTemplate for TpotBottleneckTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((_, tpot)) = signal_and_value(ctx) {
            format!("TPOT p95 is {tpot:.2}s — each output token is taking too long")
        } else {
            GenericTemplate.summary(ctx)
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let Some((_, tpot)) = signal_and_value(ctx) else {
            return GenericTemplate.evidence(ctx);
        };
        let mut lines = vec![format!("TPOT p95: {tpot:.3}s")];
        if let Some(generation) = ctx.graph.evaluate(Signal::GenerationTokensPerSecond) {
            lines.push(format!("Generation throughput: {generation:.1} tok/s"));
        }
        if let Some(ttft) = ctx.graph.evaluate(Signal::TtftP95Seconds) {
            lines.push(format!("TTFT p95: {ttft:.3}s"));
        }
        lines
    }
}

pub struct PrefixCacheEfficiencyTemplate;

impl FindingTemplate for PrefixCacheEfficiencyTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        if let Some((_, hit_rate)) = signal_and_value(ctx) {
            format!("Prefix cache hit rate is {}%", hit_rate * 100.0)
        } else {
            GenericTemplate.summary(ctx)
        }
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let Some((_, hit_rate)) = signal_and_value(ctx) else {
            return GenericTemplate.evidence(ctx);
        };
        vec![format!("Prefix cache hit rate: {}%", hit_rate * 100.0)]
    }
}

pub struct ReplicaImbalanceTemplate;

impl FindingTemplate for ReplicaImbalanceTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Load is unevenly distributed across replicas".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let replicas = ctx.graph.per_replica(Signal::NumRequestsRunning, None);
        if replicas.len() < 2 {
            return vec!["Replica imbalance detected".into()];
        }
        let max_replica = replicas
            .iter()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        let min_replica = replicas
            .iter()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(name, _)| name.clone())
            .unwrap_or_default();
        let max_value = replicas.get(&max_replica).copied().unwrap_or(0.0);
        let min_value = replicas.get(&min_replica).copied().unwrap_or(0.0);
        let ratio = if min_value > 0.0 {
            max_value / min_value
        } else {
            f64::INFINITY
        };
        vec![format!(
            "running {max_replica}={max_value:.0} vs {min_replica}={min_value:.0} (ratio {ratio:.1}x)"
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn base_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.5)]),
            request_error_total: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            request_abort_total: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            request_success_total: MetricSeries::from_samples(vec![MetricSample::new(18.0)]),
            ..Default::default()
        }
    }

    #[test]
    fn generic_template_formats_signal() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(12.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::NumRequestsRunning, 12.0);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let t = GenericTemplate;
        assert_eq!(t.summary(&ctx), "num_requests_running is elevated");
        assert_eq!(t.evidence(&ctx), vec!["num_requests_running = 12.0000"]);
    }

    #[test]
    fn queue_pressure_summary_includes_count() {
        let snapshot = base_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::NumRequestsWaiting, 8.0);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let t = QueuePressureTemplate;
        assert_eq!(t.summary(&ctx), "8 requests are waiting in the queue");
    }

    #[test]
    fn queue_latency_summary_includes_seconds() {
        let snapshot = base_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::QueueTimeP95Seconds, 2.5);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let t = QueueLatencyTemplate;
        assert!(t.summary(&ctx).contains("2.50s"));
    }

    #[test]
    fn kv_cache_summary_uses_percentage() {
        let snapshot = base_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::KvCacheUsagePerc, 0.92);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let t = KvCachePressureTemplate;
        assert_eq!(t.summary(&ctx), "GPU KV cache usage is at 92%");
    }

    #[test]
    fn preemption_evidence_reads_cache_from_graph_not_state() {
        // State value is the preemption count; usage must come from the graph.
        let snapshot = MetricSeriesSnapshot {
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.85)]),
            num_preemptions_total: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::NumPreemptionsTotal, 5.0);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let evidence = PreemptionPressureTemplate.evidence(&ctx);
        assert!(evidence.contains(&"KV cache usage: 85%".to_string()));
        assert!(evidence.contains(&"Preemptions total: 5".to_string()));
        // The old bug printed the preemption count as a percentage.
        assert!(!evidence.iter().any(|line| line.contains("500%")));
    }

    #[test]
    fn error_rate_template_computes_rates() {
        let snapshot = base_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::RequestErrorTotal, 1.0);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let t = ErrorRateTemplate;
        let evidence = t.evidence(&ctx);
        assert!(evidence[0].contains("5.0%"));
    }

    #[test]
    fn replica_imbalance_template_finds_extremes() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![
                MetricSample::new(10.0).with_label("pod", "a"),
                MetricSample::new(2.0).with_label("pod", "b"),
            ]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let state = DiagnosisState::Stressed(Signal::ReplicaRunningImbalance, 5.0);
        let ctx = TemplateContext {
            graph: &graph,
            state: &state,
        };
        let t = ReplicaImbalanceTemplate;
        let evidence = t.evidence(&ctx);
        assert!(evidence[0].contains("a=10"));
        assert!(evidence[0].contains("b=2"));
    }
}
