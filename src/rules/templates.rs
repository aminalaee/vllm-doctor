//! Shared machinery for per-rule finding templates. Each rule defines its own
//! `FindingTemplate` in its module; this holds only the trait, the context it
//! receives, and a generic fallback.
use crate::config::Config;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};

    #[test]
    fn generic_template_formats_signal() {
        let snapshot = MetricSeriesSnapshot {
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(12.0)]),
            ..Default::default()
        };
        let graph = SignalGraph::new(&snapshot);
        let config = Config::default();
        let ctx = TemplateContext {
            graph: &graph,
            config: &config,
            signal: Signal::NumRequestsRunning,
            value: 12.0,
        };
        let t = GenericTemplate;
        assert_eq!(t.summary(&ctx), "num_requests_running is elevated");
        assert_eq!(t.evidence(&ctx), vec!["num_requests_running = 12.0000"]);
    }
}
