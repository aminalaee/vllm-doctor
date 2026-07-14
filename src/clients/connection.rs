use std::path::PathBuf;

use reqwest::Client as HttpClient;
use reqwest::header::HeaderMap;

use super::error::ClientError;

#[derive(Debug, Clone, Default)]
pub struct ConnectionOptions {
    pub headers: Vec<(String, String)>,
    pub ca_cert: Option<PathBuf>,
}

impl ConnectionOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_ca_cert(mut self, path: impl Into<PathBuf>) -> Self {
        self.ca_cert = Some(path.into());
        self
    }
}

pub fn build_http_client(
    timeout: f64,
    opts: &ConnectionOptions,
) -> Result<HttpClient, ClientError> {
    let mut builder = HttpClient::builder().timeout(std::time::Duration::from_secs_f64(timeout));

    if !opts.headers.is_empty() {
        let mut header_map = HeaderMap::new();
        for (name, value) in &opts.headers {
            let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| ClientError::InvalidHeader(name.clone()))?;
            let header_value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| ClientError::InvalidHeader(name.clone()))?;
            header_map.insert(header_name, header_value);
        }
        builder = builder.default_headers(header_map);
    }

    if let Some(ref ca_path) = opts.ca_cert {
        let pem = std::fs::read(ca_path)
            .map_err(|e| ClientError::CaCert(format!("cannot read {}: {e}", ca_path.display())))?;
        let certs = reqwest::Certificate::from_pem_bundle(&pem).map_err(|e| {
            ClientError::CaCert(format!("invalid PEM in {}: {e}", ca_path.display()))
        })?;
        // `from_pem_bundle` tolerates non-PEM content and yields no certs, so an
        // unusable file would otherwise be silently ignored — surface it.
        if certs.is_empty() {
            return Err(ClientError::CaCert(format!(
                "no certificates found in {}",
                ca_path.display()
            )));
        }
        for cert in certs {
            builder = builder.add_root_certificate(cert);
        }
    }

    builder
        .build()
        .map_err(|e| ClientError::Connection { source: e })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clients::Client;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn header_sent_on_probe_and_fetch() {
        let server = MockServer::start().await;

        // With auth header → 200 with scrape content.
        Mock::given(method("GET"))
            .and(path("/metrics"))
            .and(header("authorization", "Bearer secret"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(
                        "# TYPE vllm:num_requests_running gauge\n\
                         vllm:num_requests_running 10.0\n",
                    )
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;

        let opts = ConnectionOptions::new().with_header("Authorization", "Bearer secret");
        let url = format!("{}/metrics", server.uri());
        let resolved = super::super::resolve_client(url, 1.0, &opts).await.unwrap();
        assert!(matches!(resolved, super::super::ResolvedClient::Scrape(_)));

        if let super::super::ResolvedClient::Scrape(scrape) = resolved {
            let samples = scrape.query("vllm:num_requests_running").await.unwrap();
            assert_eq!(samples.len(), 1);
        }
    }

    #[test]
    fn build_client_with_valid_header() {
        let opts = ConnectionOptions::new().with_header("X-Custom", "value");
        let client = build_http_client(1.0, &opts);
        assert!(client.is_ok());
    }

    #[test]
    fn build_client_rejects_invalid_header_name() {
        let opts = ConnectionOptions::new().with_header("invalid header", "value");
        let err = build_http_client(1.0, &opts).unwrap_err();
        assert!(matches!(err, ClientError::InvalidHeader(_)));
    }

    #[test]
    fn build_client_rejects_invalid_header_value() {
        let opts = ConnectionOptions::new().with_header("X-Test", "bad\x00value");
        let err = build_http_client(1.0, &opts).unwrap_err();
        assert!(matches!(err, ClientError::InvalidHeader(_)));
    }

    #[test]
    fn build_client_errors_on_missing_ca_file() {
        let opts = ConnectionOptions::new().with_ca_cert("/nonexistent/path.pem");
        let err = build_http_client(1.0, &opts).unwrap_err();
        assert!(matches!(err, ClientError::CaCert(ref msg) if msg.contains("cannot read")));
    }

    #[test]
    fn build_client_errors_on_ca_file_without_certs() {
        // Non-PEM content parses to zero certificates; a provided but unusable
        // CA file must error rather than silently trust nothing.
        let dir = std::env::temp_dir();
        let path = dir.join("vllm_doctor_test_invalid.pem");
        std::fs::write(&path, b"\x00\x01\x02\x03 not a cert").unwrap();
        let opts = ConnectionOptions::new().with_ca_cert(&path);
        let err = build_http_client(1.0, &opts).unwrap_err();
        std::fs::remove_file(&path).ok();
        assert!(
            matches!(err, ClientError::CaCert(ref msg) if msg.contains("no certificates found"))
        );
    }

    #[test]
    fn build_client_with_valid_pem_succeeds() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/valid-ca.pem");
        let opts = ConnectionOptions::new().with_ca_cert(path);
        assert!(build_http_client(1.0, &opts).is_ok());
    }

    #[test]
    fn connection_options_builder() {
        let opts = ConnectionOptions::new()
            .with_header("Authorization", "Bearer token")
            .with_header("X-Custom", "value")
            .with_ca_cert("/path/to/cert.pem");
        assert_eq!(opts.headers.len(), 2);
        assert_eq!(opts.headers[0].0, "Authorization");
        assert_eq!(opts.headers[1].0, "X-Custom");
        assert_eq!(
            opts.ca_cert.as_deref(),
            Some(std::path::Path::new("/path/to/cert.pem"))
        );
    }

    #[test]
    fn default_options_have_no_headers_or_ca_cert() {
        let opts = ConnectionOptions::default();
        assert!(opts.headers.is_empty());
        assert!(opts.ca_cert.is_none());
    }
}
