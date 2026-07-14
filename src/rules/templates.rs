//! Per-rule finding templates.
use crate::config::Config;
use crate::rules::error_rate;
use crate::rules::kv_cache_pressure;
use crate::rules::low_throughput;
use crate::rules::preemption_pressure;
use crate::rules::queue_latency;
use crate::rules::queue_pressure;
use crate::signals::{Signal, SignalGraph};

/// Context provided to a template when formatting a finding: the driving signal
/// and its value, plus the graph for pulling corroborating signals and the
/// config for threshold values.
pub struct TemplateContext<'a> {
    pub graph: &'a SignalGraph<'a>,
    pub config: &'a Config,
    pub signal: Signal,
    pub value: f64,
}

pub trait FindingTemplate: Sync {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String;
    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String>;
}

/// Generic fallback template used when a rule does not provide a custom one.
pub struct GenericTemplate;

impl FindingTemplate for GenericTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        let signal = ctx.signal;
        format!("{signal} is elevated")
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let signal = ctx.signal;
        let value = ctx.value;
        vec![format!("{signal} = {value:.4}")]
    }
}

pub struct QueuePressureTemplate;

impl FindingTemplate for QueuePressureTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Requests are queuing faster than the server can process them.".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let waiting = ctx.value;
        let cfg = &ctx.config.rules.queue_pressure;
        let mut lines = vec![format!(
            "Waiting requests: {waiting:.0} (threshold: {high_waiting})",
            high_waiting = cfg.high_waiting,
        )];
        if let Some(running) = queue_pressure::running_high(ctx.graph, cfg) {
            lines.push(format!(
                "Running requests: {running:.0} (threshold: {high_running})",
                high_running = cfg.high_running,
            ));
        }
        lines
    }
}

pub struct QueueLatencyTemplate;

impl FindingTemplate for QueueLatencyTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        let queue_time = ctx.value;
        format!(
            "Requests are waiting {queue_time:.2}s (p95) in the queue before prefill begins \
             — the server cannot admit requests fast enough."
        )
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let queue_time = ctx.value;
        let cfg = &ctx.config.rules.queue_latency;
        let mut lines = vec![format!(
            "Queue time p95: {queue_time:.3}s (threshold: {high}s)",
            high = cfg.high_queue_time_p95,
        )];
        if let Some(waiting) = queue_latency::waiting_backlog(ctx.graph) {
            lines.push(format!("Waiting requests: {}", waiting as i64));
        }
        lines
    }
}

pub struct KvCachePressureTemplate;

impl FindingTemplate for KvCachePressureTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        let cache = ctx.value;
        format!(
            "GPU KV cache at {:.0}% — new requests cannot be admitted until sequences complete.",
            cache * 100.0,
        )
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let cache = ctx.value;
        let cfg = &ctx.config.rules.kv_cache_pressure;
        let mut lines = vec![format!(
            "GPU KV cache usage: {usage} (threshold: {threshold})",
            usage = format!("{:.0}%", cache * 100.0),
            threshold = format!("{:.0}%", cfg.high_cache_usage * 100.0),
        )];
        if let Some(waiting) = kv_cache_pressure::waiting_backlog(ctx.graph) {
            lines.push(format!(
                "Waiting requests: {waiting:.0} (blocked by full cache)"
            ));
        }
        lines
    }
}

pub struct PreemptionPressureTemplate;

impl FindingTemplate for PreemptionPressureTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        let preemptions =
            Some(ctx.value).or_else(|| ctx.graph.evaluate(Signal::NumPreemptionsTotal));
        let Some(preemptions) = preemptions else {
            return GenericTemplate.summary(ctx);
        };
        format!(
            "vLLM has preempted {preemptions:.0} sequences — \
             KV cache exhaustion is forcing sequences to be re-computed."
        )
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let preemptions =
            Some(ctx.value).or_else(|| ctx.graph.evaluate(Signal::NumPreemptionsTotal));
        let Some(preemptions) = preemptions else {
            return GenericTemplate.evidence(ctx);
        };
        let mut lines = vec![format!("Preemptions total: {preemptions:.0}")];
        let cfg = &ctx.config.rules.preemption_pressure;
        if let Some(cache) = preemption_pressure::cache_high(ctx.graph, cfg) {
            lines.push(format!(
                "GPU KV cache usage: {:.0}% (threshold: {:.0}%)",
                cache * 100.0,
                cfg.high_cache_usage * 100.0,
            ));
        }
        lines
    }
}

pub struct PrefixCacheEfficiencyTemplate;

impl FindingTemplate for PrefixCacheEfficiencyTemplate {
    fn summary(&self, ctx: &TemplateContext<'_>) -> String {
        let hit_rate = ctx.value;
        format!(
            "Prefix cache hit rate is {:.0}% — repeated prompt prefixes are not being reused, \
             causing redundant prefill computation.",
            hit_rate * 100.0,
        )
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let hit_rate = ctx.value;
        vec![format!(
            "Prefix cache hit rate: {}",
            format!("{:.0}%", hit_rate * 100.0)
        )]
    }
}

pub struct ErrorRateTemplate;

impl FindingTemplate for ErrorRateTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Server is returning errors or clients are aborting at an elevated rate.".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let cfg = &ctx.config.rules.error_rate;
        let Some((errors, aborts, success)) = error_rate::request_totals(ctx.graph) else {
            return vec![];
        };
        let total = errors + aborts + success;
        if total == 0.0 {
            return vec![];
        }
        let error_rate = errors / total;
        let abort_rate = aborts / total;
        let mut lines = vec![];
        if error_rate::error_rate_high(ctx.graph, cfg).is_some() {
            lines.push(format!(
                "Error rate: {:.1}% ({errors:.0} errors out of {total:.0} requests, \
                 threshold: {:.1}%)",
                error_rate * 100.0,
                cfg.high_error_rate * 100.0,
            ));
        }
        if error_rate::abort_rate_high(ctx.graph, cfg).is_some() {
            lines.push(format!(
                "Abort rate: {:.1}% ({aborts:.0} aborts out of {total:.0} requests, \
                 threshold: {:.1}%)",
                abort_rate * 100.0,
                cfg.high_abort_rate * 100.0,
            ));
        }
        lines
    }
}

pub struct LowThroughputTemplate;

impl FindingTemplate for LowThroughputTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Server is processing requests below expected throughput with no queue pressure.".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let cfg = &ctx.config.rules.low_throughput;
        let mut lines = vec![];
        if let Some(prompt) = low_throughput::prompt_low(ctx.graph, cfg) {
            lines.push(format!(
                "Prompt tokens/s: {prompt:.1} (threshold: {threshold:.1})",
                threshold = cfg.low_prompt_tps,
            ));
        }
        if let Some(gen_tps) = low_throughput::gen_low(ctx.graph, cfg) {
            lines.push(format!(
                "Generation tokens/s: {gen_tps:.1} (threshold: {threshold:.1})",
                threshold = cfg.low_gen_tps,
            ));
        }
        if let Some(running) = low_throughput::running_low(ctx.graph, cfg) {
            lines.push(format!("Requests running: {running:.0}"));
        }
        lines
    }
}

pub struct TtftBottleneckTemplate;

impl FindingTemplate for TtftBottleneckTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Requests are waiting too long before receiving the first token. \
         This typically indicates prefill or queue pressure."
            .into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let ttft = ctx.value;
        let mut lines = vec![format!("TTFT p95: {ttft:.3}s")];
        if let Some(tpot) = ctx.graph.evaluate(Signal::TpotP95Seconds) {
            if tpot.is_finite() {
                lines.push(format!("TPOT p95: {tpot:.3}s"));
            }
        }
        if let Some(waiting) = ctx.graph.evaluate(Signal::NumRequestsWaiting) {
            if waiting > 0.0 {
                lines.push(format!("Waiting requests: {}", waiting as i64));
            }
        }
        lines
    }
}

pub struct TpotBottleneckTemplate;

impl FindingTemplate for TpotBottleneckTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Each output token is taking too long to generate. \
         This typically indicates GPU decode saturation or memory bandwidth pressure."
            .into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let tpot = ctx.value;
        let mut lines = vec![format!("TPOT p95: {tpot:.3}s")];
        if let Some(gen_tps) = ctx.graph.evaluate(Signal::GenerationTokensPerSecond) {
            if gen_tps.is_finite() {
                lines.push(format!("Generation throughput: {gen_tps:.1} tok/s"));
            }
        }
        if let Some(ttft) = ctx.graph.evaluate(Signal::TtftP95Seconds) {
            if ttft.is_finite() {
                lines.push(format!("TTFT p95: {ttft:.3}s"));
            }
        }
        lines
    }
}

pub struct ReplicaImbalanceTemplate;

impl FindingTemplate for ReplicaImbalanceTemplate {
    fn summary(&self, _ctx: &TemplateContext<'_>) -> String {
        "Load is unevenly distributed across replicas".into()
    }

    fn evidence(&self, ctx: &TemplateContext<'_>) -> Vec<String> {
        let cfg = &ctx.config.rules.replica_imbalance;
        let mut lines = vec![];
        for model in ctx.graph.models() {
            if let Some(evidence) =
                crate::rules::replica_imbalance::model_imbalance(ctx.graph, model.as_deref(), cfg)
            {
                let prefix = model
                    .as_deref()
                    .map(|m| format!("{m}: "))
                    .unwrap_or_default();
                lines.push(format!("{prefix}{}", evidence.parts.join("; ")));
            }
        }
        if lines.is_empty() {
            lines.push("Replica imbalance detected".into());
        }
        lines
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

    fn default_config() -> &'static Config {
        use std::sync::OnceLock;
        static CFG: OnceLock<Config> = OnceLock::new();
        CFG.get_or_init(Config::default)
    }

    fn ctx_for<'a>(graph: &'a SignalGraph<'a>, signal: Signal, value: f64) -> TemplateContext<'a> {
        TemplateContext {
            graph,
            config: default_config(),
            signal,
            value,
        }
    }

    #[test]
    fn generic_template_formats_signal() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(12.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::NumRequestsRunning, 12.0);
        let t = GenericTemplate;
        assert_eq!(t.summary(&ctx), "num_requests_running is elevated");
        assert_eq!(t.evidence(&ctx), vec!["num_requests_running = 12.0000"]);
    }

    #[test]
    fn queue_pressure_output() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(60.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::NumRequestsWaiting, 8.0);
        let t = QueuePressureTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Requests are queuing faster than the server can process them."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "Waiting requests: 8 (threshold: 5)");
        assert_eq!(evidence[1], "Running requests: 60 (threshold: 50)");
    }

    #[test]
    fn queue_pressure_no_running_line_when_below_threshold() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(10.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::NumRequestsWaiting, 8.0);
        let t = QueuePressureTemplate;
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence, vec!["Waiting requests: 8 (threshold: 5)"]);
    }

    #[test]
    fn queue_latency_output() {
        let snapshot = MetricSeriesSnapshot {
            queue_time_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(2.5)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(3.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::QueueTimeP95Seconds, 2.5);
        let t = QueueLatencyTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Requests are waiting 2.50s (p95) in the queue before prefill begins \
             — the server cannot admit requests fast enough."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "Queue time p95: 2.500s (threshold: 1s)");
        assert_eq!(evidence[1], "Waiting requests: 3");
    }

    #[test]
    fn kv_cache_output() {
        let snapshot = MetricSeriesSnapshot {
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.92)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(4.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::KvCacheUsagePerc, 0.92);
        let t = KvCachePressureTemplate;
        assert_eq!(
            t.summary(&ctx),
            "GPU KV cache at 92% — new requests cannot be admitted until sequences complete."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "GPU KV cache usage: 92% (threshold: 90%)");
        assert_eq!(evidence[1], "Waiting requests: 4 (blocked by full cache)");
    }

    #[test]
    fn preemption_output_order() {
        let snapshot = MetricSeriesSnapshot {
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.85)]),
            num_preemptions_total: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::NumPreemptionsTotal, 5.0);
        let t = PreemptionPressureTemplate;
        assert_eq!(
            t.summary(&ctx),
            "vLLM has preempted 5 sequences — \
             KV cache exhaustion is forcing sequences to be re-computed."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "Preemptions total: 5");
        assert_eq!(evidence[1], "GPU KV cache usage: 85% (threshold: 80%)");
    }

    #[test]
    fn preemption_no_cache_line_when_below_threshold() {
        let snapshot = MetricSeriesSnapshot {
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.50)]),
            num_preemptions_total: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::NumPreemptionsTotal, 5.0);
        let t = PreemptionPressureTemplate;
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence, vec!["Preemptions total: 5"]);
    }

    #[test]
    fn prefix_cache_output() {
        let snapshot = MetricSeriesSnapshot {
            prefix_cache_hit_rate: MetricSeries::from_samples(vec![MetricSample::new(0.12)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::PrefixCacheHitRate, 0.12);
        let t = PrefixCacheEfficiencyTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Prefix cache hit rate is 12% — repeated prompt prefixes are not being reused, \
             causing redundant prefill computation."
        );
        assert_eq!(t.evidence(&ctx), vec!["Prefix cache hit rate: 12%"]);
    }

    #[test]
    fn error_rate_output() {
        // 1 error, 1 abort, 18 success -> total 20.
        // error_rate = 0.05, abort_rate = 0.05; high_error_rate = 0.05 (>=),
        // high_abort_rate = 0.10 (abort not high).
        let snapshot = base_snapshot();
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::RequestErrorTotal, 1.0);
        let t = ErrorRateTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Server is returning errors or clients are aborting at an elevated rate."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(
            evidence[0],
            "Error rate: 5.0% (1 errors out of 20 requests, threshold: 5.0%)"
        );
    }

    #[test]
    fn error_rate_includes_abort_line_when_high() {
        let snapshot = MetricSeriesSnapshot {
            request_error_total: MetricSeries::from_samples(vec![MetricSample::new(3.0)]),
            request_abort_total: MetricSeries::from_samples(vec![MetricSample::new(3.0)]),
            request_success_total: MetricSeries::from_samples(vec![MetricSample::new(14.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::RequestErrorTotal, 3.0);
        let t = ErrorRateTemplate;
        let evidence = t.evidence(&ctx);
        // total = 20, error_rate = 0.15, abort_rate = 0.15.
        assert_eq!(
            evidence[0],
            "Error rate: 15.0% (3 errors out of 20 requests, threshold: 5.0%)"
        );
        assert_eq!(
            evidence[1],
            "Abort rate: 15.0% (3 aborts out of 20 requests, threshold: 10.0%)"
        );
    }

    #[test]
    fn low_throughput_output() {
        let snapshot = MetricSeriesSnapshot {
            prompt_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(5.0)]),
            generation_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(20.0)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::PromptTokensPerSecond, 5.0);
        let t = LowThroughputTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Server is processing requests below expected throughput with no queue pressure."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "Prompt tokens/s: 5.0 (threshold: 10.0)");
        assert_eq!(evidence[1], "Generation tokens/s: 20.0 (threshold: 50.0)");
        assert_eq!(evidence[2], "Requests running: 1");
    }

    #[test]
    fn ttft_bottleneck_output() {
        let snapshot = MetricSeriesSnapshot {
            ttft_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(2.5)]),
            tpot_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(0.1)]),
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(3.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::TtftP95Seconds, 2.5);
        let t = TtftBottleneckTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Requests are waiting too long before receiving the first token. \
             This typically indicates prefill or queue pressure."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "TTFT p95: 2.500s");
        assert_eq!(evidence[1], "TPOT p95: 0.100s");
        assert_eq!(evidence[2], "Waiting requests: 3");
    }

    #[test]
    fn tpot_bottleneck_output() {
        let snapshot = MetricSeriesSnapshot {
            tpot_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(0.4)]),
            generation_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(30.0)]),
            ttft_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(1.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let ctx = ctx_for(&graph, Signal::TpotP95Seconds, 0.4);
        let t = TpotBottleneckTemplate;
        assert_eq!(
            t.summary(&ctx),
            "Each output token is taking too long to generate. \
             This typically indicates GPU decode saturation or memory bandwidth pressure."
        );
        let evidence = t.evidence(&ctx);
        assert_eq!(evidence[0], "TPOT p95: 0.400s");
        assert_eq!(evidence[1], "Generation throughput: 30.0 tok/s");
        assert_eq!(evidence[2], "TTFT p95: 1.000s");
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
        let ctx = ctx_for(&graph, Signal::ReplicaRunningImbalance, 5.0);
        let t = ReplicaImbalanceTemplate;
        let evidence = t.evidence(&ctx);
        assert!(evidence[0].contains("a=10"));
        assert!(evidence[0].contains("b=2"));
    }
}
