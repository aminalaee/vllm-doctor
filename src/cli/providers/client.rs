//! Generic provider backed by any `Client`: builds the HTTP client and collects
//! a fresh snapshot per request. Shared by the scrape and Prometheus providers.
use std::sync::Arc;

use super::{Provider, ProviderError, ProviderMetadata};
use crate::cli::clients::Client;
use crate::cli::clients::connection::{ConnectionOptions, build_http_client};
use crate::cli::collector::collect;
use crate::core::metrics::MetricSeriesSnapshot;
use crate::core::models::MetricsSource;

/// A `Provider` backed by any `Client`.
///
/// The `id` string identifies the provider kind in `metadata()` (e.g.
/// `"scrape"` or `"prometheus"`). `metrics_source` records the transport kind
/// so callers don't have to re-derive it from `id`.
#[derive(Debug)]
pub struct ClientProvider<C> {
    client: Arc<C>,
    since: String,
    model: Option<String>,
    endpoint: String,
    id: &'static str,
    metrics_source: MetricsSource,
}

impl<C: Client + Send + Sync + 'static> ClientProvider<C> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: impl Into<String>,
        timeout: f64,
        opts: &ConnectionOptions,
        since: impl Into<String>,
        model: Option<impl Into<String>>,
        id: &'static str,
        metrics_source: MetricsSource,
        build_client: impl FnOnce(
            String,
            reqwest::Client,
        ) -> Result<C, crate::cli::clients::error::ClientError>,
    ) -> Result<Self, ProviderError> {
        let endpoint = url.into();
        let http_client = build_http_client(timeout, opts)?;
        let client = Arc::new(build_client(endpoint.clone(), http_client)?);
        Ok(Self {
            client,
            since: since.into(),
            model: model.map(Into::into),
            endpoint,
            id,
            metrics_source,
        })
    }
}

#[async_trait::async_trait]
impl<C: Client + Send + Sync + 'static> Provider for ClientProvider<C> {
    async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
        collect(self.client.clone(), &self.since, self.model.as_deref())
            .await
            .map_err(ProviderError::from)
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.id,
            endpoint: self.endpoint.clone(),
            metrics_source: self.metrics_source,
        }
    }
}
