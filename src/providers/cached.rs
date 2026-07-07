//! Generic cached provider: holds a client, cache, and fetch logic shared by
//! the scrape and Prometheus providers.
use std::sync::Arc;
use std::time::Duration;

use reqwest::Client as HttpClient;

use super::{Provider, ProviderError, ProviderMetadata, RequestCycleCache};
use crate::clients::Client;
use crate::collector::collect;
use crate::metrics::MetricSeriesSnapshot;

/// A `Provider` backed by any `Client` and a `RequestCycleCache`.
///
/// The `id` string identifies the provider kind in `metadata()` (e.g.
/// `"scrape"` or `"prometheus"`).
#[derive(Debug)]
pub struct CachedProvider<C> {
    client: Arc<C>,
    cache: RequestCycleCache,
    since: String,
    model: Option<String>,
    endpoint: String,
    id: &'static str,
}

impl<C: Client + Send + Sync + 'static> CachedProvider<C> {
    /// Build a provider with a shared connection pool.
    pub fn new(
        url: impl Into<String>,
        timeout: f64,
        since: impl Into<String>,
        model: Option<impl Into<String>>,
        id: &'static str,
        build_client: impl FnOnce(String, HttpClient) -> Result<C, crate::clients::error::ClientError>,
    ) -> Result<Self, ProviderError> {
        Self::with_cache_ttl(
            url,
            timeout,
            since,
            model,
            id,
            super::DEFAULT_CACHE_TTL,
            build_client,
        )
    }

    /// Build a provider with a custom cache TTL.
    pub fn with_cache_ttl(
        url: impl Into<String>,
        timeout: f64,
        since: impl Into<String>,
        model: Option<impl Into<String>>,
        id: &'static str,
        ttl: Duration,
        build_client: impl FnOnce(String, HttpClient) -> Result<C, crate::clients::error::ClientError>,
    ) -> Result<Self, ProviderError> {
        let endpoint = url.into();
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs_f64(timeout))
            .build()
            .map_err(crate::clients::error::ClientError::from)?;
        let client = Arc::new(build_client(endpoint.clone(), http_client)?);
        Ok(Self {
            client,
            cache: RequestCycleCache::new(ttl),
            since: since.into(),
            model: model.map(Into::into),
            endpoint,
            id,
        })
    }
}

#[async_trait::async_trait]
impl<C: Client + Send + Sync + 'static> Provider for CachedProvider<C> {
    async fn fetch_snapshot(&self) -> Result<MetricSeriesSnapshot, ProviderError> {
        if let Some(snapshot) = self.cache.get() {
            return Ok(snapshot);
        }
        let snapshot = collect(self.client.clone(), &self.since, self.model.as_deref()).await?;
        self.cache.update(snapshot.clone());
        Ok(snapshot)
    }

    fn metadata(&self) -> ProviderMetadata {
        ProviderMetadata {
            id: self.id,
            endpoint: self.endpoint.clone(),
        }
    }
}
