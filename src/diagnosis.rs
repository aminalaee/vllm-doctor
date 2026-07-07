//! Diagnosis orchestration: fetch a metric snapshot and evaluate the rule
//! registry into a single, ordered result.
use crate::config::Config;
use crate::models::{ClientMode, DiagnosisContext, DiagnosisResult};
use crate::providers::{Provider, ProviderError};
use crate::rules::RuleRegistry;

/// Fetch a snapshot from `provider`, run every rule, and assemble the result.
///
/// The context records the requested window, the optional model filter, and the
/// client mode inferred from the provider.
pub async fn diagnose(
    provider: &dyn Provider,
    registry: &RuleRegistry,
    since: &str,
    model: Option<&str>,
    config: &Config,
) -> Result<DiagnosisResult, ProviderError> {
    let snapshot = provider.fetch_snapshot().await?;
    let checks = registry.run_all(&snapshot, config);

    let client_mode = match provider.metadata().id {
        "scrape" => ClientMode::Scrape,
        _ => ClientMode::Prometheus,
    };
    let mut context = DiagnosisContext::new(since).with_client_mode(client_mode);
    if let Some(model) = model {
        context = context.with_model_name(model);
    }

    Ok(DiagnosisResult {
        context,
        metric_series: snapshot,
        checks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::error::ClientError;
    use crate::config::Config;
    use crate::metrics::MetricSeriesSnapshot;
    use crate::metrics::series::{MetricSample, MetricSeries};
    use crate::models::{Health, Severity};
    use crate::providers::ProviderMetadata;
    use crate::rules::build_registry;

    struct StubProvider {
        id: &'static str,
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
            }
        }
    }

    struct FailingProvider;

    #[async_trait::async_trait]
    impl Provider for FailingProvider {
        async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
            Err(ProviderError::Fetch(ClientError::Query("forced".into())))
        }

        fn metadata(&self) -> ProviderMetadata {
            ProviderMetadata {
                id: "failing",
                endpoint: "nowhere".into(),
            }
        }
    }

    fn pressured_snapshot() -> MetricSeriesSnapshot {
        MetricSeriesSnapshot {
            num_requests_waiting: MetricSeries::from_samples(vec![MetricSample::new(8.0)]),
            num_requests_running: MetricSeries::from_samples(vec![MetricSample::new(60.0)]),
            kv_cache_usage_perc: MetricSeries::from_samples(vec![MetricSample::new(0.95)]),
            // High error rate fires a critical finding (80 / 1110 > 5% threshold).
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
            snapshot: pressured_snapshot(),
        };
        let config = Config::default();
        let registry = build_registry(&config);
        let result = diagnose(&provider, &registry, "10m", Some("llama"), &config)
            .await
            .unwrap();

        assert_eq!(result.context.since, "10m");
        assert_eq!(result.context.model_name, Some("llama".to_string()));
        assert_eq!(result.context.client_mode, ClientMode::Scrape);
        assert_eq!(result.checks.len(), 10);
        assert_ne!(result.metric_series, MetricSeriesSnapshot::default());
    }

    #[tokio::test]
    async fn client_mode_follows_provider_id() {
        let config = Config::default();
        let registry = build_registry(&config);
        for (id, expected) in [
            ("scrape", ClientMode::Scrape),
            ("prometheus", ClientMode::Prometheus),
        ] {
            let provider = StubProvider {
                id,
                snapshot: MetricSeriesSnapshot::default(),
            };
            let result = diagnose(&provider, &registry, "5m", None, &config)
                .await
                .unwrap();
            assert_eq!(result.context.client_mode, expected);
            assert_eq!(result.context.model_name, None);
        }
    }

    #[tokio::test]
    async fn checks_are_ordered_worst_first() {
        let provider = StubProvider {
            id: "prometheus",
            snapshot: pressured_snapshot(),
        };
        let config = Config::default();
        let registry = build_registry(&config);
        let result = diagnose(&provider, &registry, "5m", None, &config)
            .await
            .unwrap();

        // Findings precede non-firing checks, and severities are non-decreasing.
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
        // The KV cache pressure rule fires critical on 95% usage.
        assert_eq!(result.health(), Health::Critical);
    }

    #[tokio::test]
    async fn propagates_provider_errors() {
        let config = Config::default();
        let registry = build_registry(&config);
        let err = diagnose(&FailingProvider, &registry, "5m", None, &config)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("forced"));
    }
}
