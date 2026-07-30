//! Diagnosis orchestration: fetch a metric snapshot and evaluate the rule
//! registry into a single, ordered result.
use crate::core::config::CoreConfig as Config;

use crate::core::models::{DiagnosisContext, DiagnosisResult, TargetMetadata};
use crate::core::providers::{Provider, ProviderError};
use crate::core::rules::RuleRegistry;

/// Fetch a snapshot from `provider`, run every rule, and assemble the result.
///
/// The context records the requested window, the optional model filter, the
/// metrics source reported by the provider, and the target metadata.
pub async fn diagnose(
    provider: &dyn Provider,
    registry: &RuleRegistry,
    since: &str,
    model: Option<&str>,
    target: &TargetMetadata,
    config: &Config,
) -> Result<DiagnosisResult, ProviderError> {
    let snapshot = provider.fetch_snapshot().await?;
    let checks = registry.run_all(&snapshot, config);

    let metrics_source = provider.metadata().metrics_source;
    let mut context = DiagnosisContext::new(since)
        .with_metrics_source(metrics_source)
        .with_target(target.clone());
    if let Some(model) = model {
        context = context.with_model_name(model);
    }

    let mut result = DiagnosisResult::new(context, snapshot, checks);
    result.assessment = crate::core::assessment::assess(&result);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::core::config::CoreConfig as Config;

    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::metrics::series::{MetricSample, MetricSeries};
    use crate::core::models::{Health, MetricsSource, Severity, TargetMetadata};
    use crate::core::providers::ProviderMetadata;
    use crate::core::rules::build_registry;

    struct StubProvider {
        id: &'static str,
        metrics_source: MetricsSource,
        snapshot: MetricSeriesSnapshot,
    }

    #[async_trait::async_trait]
    impl Provider for StubProvider {
        async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
            Ok(self.snapshot.clone())
        }

        fn metadata(&self) -> ProviderMetadata {
            ProviderMetadata {
                id: self.id,
                endpoint: "test://local".to_string(),
                metrics_source: self.metrics_source,
            }
        }
    }

    struct FailingProvider;

    #[async_trait::async_trait]
    impl Provider for FailingProvider {
        async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
            Err(ProviderError::Fetch(std::io::Error::other("forced").into()))
        }

        fn metadata(&self) -> ProviderMetadata {
            ProviderMetadata {
                id: "failing",
                endpoint: "nowhere".into(),
                metrics_source: MetricsSource::Prometheus,
            }
        }
    }

    fn pressured_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(60.0)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.95)]),
            request_success_total: MetricSeries::from_samples(vec![MetricSample::new(1000.0)]),
            request_error_total: MetricSeries::from_samples(vec![MetricSample::new(80.0)]),
            request_abort_total: MetricSeries::from_samples(vec![MetricSample::new(30.0)]),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn assembles_context_snapshot_and_checks() {
        let provider = StubProvider {
            id: "scrape",
            metrics_source: MetricsSource::DirectScrape,
            snapshot: pressured_snapshot(),
        };
        let config = Config::default();
        let registry = build_registry(&config);
        let result = diagnose(
            &provider,
            &registry,
            "10m",
            Some("llama"),
            &TargetMetadata::default(),
            &config,
        )
        .await
        .unwrap();

        assert_eq!(result.context.since, "10m");
        assert_eq!(result.context.model_name, Some("llama".to_string()));
        assert_eq!(result.context.metrics_source, MetricsSource::DirectScrape);
        assert_eq!(result.checks.len(), 10);
        assert_ne!(result.metric_series, MetricSeriesSnapshot::default());
    }

    #[tokio::test]
    async fn metrics_source_follows_provider_metadata() {
        let config = Config::default();
        let registry = build_registry(&config);
        for (id, metrics_source, expected) in [
            (
                "scrape",
                MetricsSource::DirectScrape,
                MetricsSource::DirectScrape,
            ),
            (
                "prometheus",
                MetricsSource::Prometheus,
                MetricsSource::Prometheus,
            ),
        ] {
            let provider = StubProvider {
                id,
                metrics_source,
                snapshot: MetricSeriesSnapshot::default(),
            };
            let result = diagnose(
                &provider,
                &registry,
                "5m",
                None,
                &TargetMetadata::default(),
                &config,
            )
            .await
            .unwrap();
            assert_eq!(result.context.metrics_source, expected);
            assert_eq!(result.context.model_name, None);
        }
    }

    #[tokio::test]
    async fn checks_are_ordered_worst_first() {
        let provider = StubProvider {
            id: "prometheus",
            metrics_source: MetricsSource::Prometheus,
            snapshot: pressured_snapshot(),
        };
        let config = Config::default();
        let registry = build_registry(&config);
        let result = diagnose(
            &provider,
            &registry,
            "5m",
            None,
            &TargetMetadata::default(),
            &config,
        )
        .await
        .unwrap();

        let mut last = (0u8, 0u8);
        let mut seen_none = false;
        for check in &result.checks {
            match &check.finding {
                Some(finding) => {
                    assert!(!seen_none, "a finding appeared after a non-firing check");
                    let severity = match finding.severity {
                        Severity::Critical => 0,
                        Severity::Warning => 1,
                        Severity::Info => 2,
                    };
                    assert!(severity >= last.0, "severity ordering regressed");
                    last = (severity, 0);
                }
                None => seen_none = true,
            }
        }
        assert_eq!(result.health(), Health::Critical);
    }

    #[tokio::test]
    async fn propagates_provider_errors() {
        let config = Config::default();
        let registry = build_registry(&config);
        let err = diagnose(
            &FailingProvider,
            &registry,
            "5m",
            None,
            &TargetMetadata::default(),
            &config,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("forced"));
    }
}
