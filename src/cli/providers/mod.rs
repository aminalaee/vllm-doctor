//! Providers: fetch metric snapshots for the diagnostic engine.
use crate::cli::clients::ConnectionOptions;

pub mod client;
pub mod prometheus;
pub mod scrape;

pub use prometheus::PrometheusProvider;
pub use scrape::ScrapeProvider;

// Re-export the provider boundary used throughout the CLI.
pub use crate::core::providers::{Provider, ProviderError, ProviderMetadata};

use crate::cli::clients::error::ClientError;

/// Convert a CLI [`ClientError`] into a core [`ProviderError`].
impl From<ClientError> for ProviderError {
    fn from(err: ClientError) -> Self {
        ProviderError::Fetch(Box::new(err))
    }
}

/// Probe `url` to choose between a raw `/metrics` scrape and the Prometheus
/// query API, then build the matching provider.
pub async fn resolve_provider(
    url: &str,
    timeout: f64,
    opts: &ConnectionOptions,
    since: &str,
    model: Option<&str>,
) -> Result<Box<dyn Provider>, ProviderError> {
    use crate::cli::clients::{ResolvedClient, resolve_client};
    let resolved = resolve_client(url, timeout, opts)
        .await
        .map_err(ProviderError::from)?;
    let provider: Box<dyn Provider> = match resolved {
        ResolvedClient::Scrape(_) => Box::new(scrape::new(url, timeout, opts, since, model)?),
        ResolvedClient::Prometheus(_) => {
            Box::new(prometheus::new(url, timeout, opts, since, model)?)
        }
    };
    Ok(provider)
}
