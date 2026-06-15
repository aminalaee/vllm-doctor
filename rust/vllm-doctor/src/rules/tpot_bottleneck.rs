//! TPOT bottleneck rule.
//!
//! Detects when time per output token (p95) exceeds the configured threshold.
//! Confidence rises when generation throughput is also low — corroborating decode
//! pressure — and when TTFT is not elevated, isolating the bottleneck to decode
//! rather than prefill or queue saturation.
use crate::config::TpotBottleneckConfig;
use crate::models::{Confidence, FindingData};
use crate::rules::Rule;
use crate::signals::{Signal, SignalGraph};

pub struct TpotBottleneckRule {
    cfg: TpotBottleneckConfig,
}

impl TpotBottleneckRule {
    pub fn new(cfg: TpotBottleneckConfig) -> Self {
        Self { cfg }
    }
}

impl Rule for TpotBottleneckRule {
    fn id(&self) -> &'static str {
        "tpot_bottleneck"
    }

    fn name(&self) -> &'static str {
        "High TPOT"
    }

    fn title(&self) -> &'static str {
        "High time per output token (TPOT)"
    }

    fn severity(&self) -> crate::models::Severity {
        crate::models::Severity::Warning
    }

    fn likely_causes(&self) -> &'static [&'static str] {
        &[
            "GPU memory bandwidth saturated during decode",
            "Too many concurrent sequences reducing per-request throughput",
            "Large model size relative to available GPU memory",
            "Insufficient tensor parallelism for current load",
        ]
    }

    fn recommendations(&self) -> &'static [&'static str] {
        &[
            "Reduce max concurrent requests (--max-num-seqs)",
            "Increase tensor parallelism to distribute decode across GPUs",
            "Enable speculative decoding to amortize decode cost",
            "Profile GPU memory bandwidth utilization",
        ]
    }

    fn related_metrics(&self) -> &'static [&'static str] {
        &[
            "tpot_p95_seconds",
            "generation_tokens_per_second",
            "ttft_p95_seconds",
        ]
    }

    fn run(&self, signals: &SignalGraph<'_>) -> Option<FindingData> {
        let tpot = signals.evaluate(Signal::TpotP95Seconds)?;
        if tpot < self.cfg.high_tpot_p95 {
            return None;
        }

        let ttft = signals.evaluate(Signal::TtftP95Seconds);
        let gen_tps = signals.evaluate(Signal::GenerationTokensPerSecond);

        let mut signals_list = vec![format!(
            "TPOT p95 ({:.2}s) exceeds threshold ({}s)",
            tpot, self.cfg.high_tpot_p95
        )];
        let mut evidence = vec![format!("TPOT p95: {:.3}s", tpot)];

        let gen_low = gen_tps.is_some_and(|v| v.is_finite() && v < self.cfg.low_gen_tokens_per_sec);
        let ttft_normal = ttft.is_some_and(|v| v.is_finite() && v < 2.0);

        if let Some(v) = gen_tps {
            evidence.push(format!("Generation throughput: {:.1} tok/s", v));
        }
        if gen_low {
            signals_list.push(format!(
                "Generation throughput ({:.1} tok/s) is low — decode is the bottleneck",
                gen_tps.unwrap_or(0.0)
            ));
        }
        if let Some(v) = ttft {
            evidence.push(format!("TTFT p95: {:.3}s", v));
        }
        if ttft_normal {
            signals_list
                .push("TTFT p95 is normal — bottleneck is in decode, not prefill".to_string());
        }

        let signal_count = 1 + usize::from(gen_low) + usize::from(ttft_normal);
        let confidence = match signal_count {
            3 => Confidence::High,
            2 => Confidence::Medium,
            _ => Confidence::Low,
        };

        Some(FindingData {
            confidence,
            summary: "Each output token is taking too long to generate. This typically indicates GPU decode saturation or memory bandwidth pressure.".to_string(),
            signals: signals_list,
            evidence,
            severity: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};

    fn rule() -> TpotBottleneckRule {
        TpotBottleneckRule::new(TpotBottleneckConfig {
            high_tpot_p95: 0.2,
            low_gen_tokens_per_sec: 50.0,
        })
    }

    fn snapshot(tpot: f64, ttft: f64, gen_tps: f64) -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            tpot_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(tpot)]),
            ttft_p95_seconds: MetricSeries::from_samples(vec![MetricSample::new(ttft)]),
            generation_tokens_per_second: MetricSeries::from_samples(vec![MetricSample::new(
                gen_tps,
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn no_finding_when_tpot_low() {
        assert!(
            rule()
                .run(&SignalGraph::new(&snapshot(0.1, 1.0, 100.0)))
                .is_none()
        );
    }

    #[test]
    fn low_confidence_when_tpot_high_only() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(0.3, 5.0, 100.0)))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::Low);
    }

    #[test]
    fn high_confidence_when_tpot_high_with_low_gen_and_normal_ttft() {
        let finding = rule()
            .run(&SignalGraph::new(&snapshot(0.3, 1.0, 10.0)))
            .unwrap();
        assert_eq!(finding.confidence, Confidence::High);
    }
}
