//! HTTP upload client for `ObservationV1` payloads.
//!
//! POSTs a serialized observation to the versioned vLLM Doctor API with Bearer
//! auth. The token is never embedded in the client struct; it is passed per
//! call so a long-lived client cannot leak it in debug output.

use reqwest::StatusCode;
use thiserror::Error;
use uuid::Uuid;

use crate::cli::config::UploadConfig;
use crate::core::observations::v1::MAX_UNCOMPRESSED_JSON_BYTES;
use crate::core::observations::v1::ObservationV1;

const API_URL: &str = "https://api.vllm.doctor";
const API_VERSION: &str = "v1";

/// Outcome of a successful upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadOutcome {
    /// The server created a new observation (201 Created).
    Created,
    /// The server deduplicated the observation against an existing one
    /// (200 OK).
    Deduplicated,
}

/// Errors that can occur during upload.
#[derive(Debug, Error)]
pub enum UploadError {
    /// The bearer token could not be resolved from the environment.
    #[error("{0}")]
    Token(#[from] crate::cli::upload::config::TokenError),
    /// The validated observation could not be serialized.
    #[error("could not serialize observation: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The diagnosis window could not be represented by the upload contract.
    #[error("invalid diagnosis window: {0}")]
    InvalidWindow(String),
    /// The diagnosis result could not be converted into an observation.
    #[error("could not build observation: {0}")]
    Build(String),
    /// The serialized payload exceeded the client-side size limit before
    /// sending.
    #[error("payload too large: serialized body is {len} bytes (limit {limit})")]
    PayloadTooLarge { len: usize, limit: usize },
    /// The server rejected the request as unauthorized (401).
    #[error("unauthorized: the upload token was rejected by the server")]
    Unauthorized,
    /// The server reported a conflict for the same event (409).
    #[error("conflict: the server reported a conflicting event for event_id {event_id}")]
    Conflict { event_id: Uuid },
    /// The server rejected the payload as too large (413).
    #[error("payload too large: the server rejected the request body")]
    ServerPayloadTooLarge,
    /// The server rejected the payload (400).
    #[error("the server rejected the observation")]
    Rejected,
    /// The server rate-limited the upload (429).
    #[error("the server rate-limited the upload")]
    RateLimited,
    /// The server returned an unexpected status code.
    #[error("unexpected response from server: {status}")]
    UnexpectedStatus { status: u16 },
    /// A network or transport error occurred.
    #[error("upload request failed: {0}")]
    Request(#[from] reqwest::Error),
}

impl UploadError {
    /// Whether this error is worth an immediate retry (network blip).
    pub fn is_retryable(&self) -> bool {
        matches!(self, UploadError::Request(_) | UploadError::RateLimited)
            || matches!(
                self,
                UploadError::UnexpectedStatus { status } if (500..=599).contains(status)
            )
    }
}

/// HTTP client for uploading observations. Reuse across upload calls to keep
/// the connection pool warm.
#[derive(Debug, Clone)]
pub struct UploadClient {
    base_url: String,
    http: reqwest::Client,
}

impl UploadClient {
    /// Build an upload client from the resolved upload config.
    pub fn new(config: &UploadConfig) -> Result<Self, UploadError> {
        Self::with_base_url(config, API_URL)
    }

    fn with_base_url(config: &UploadConfig, base_url: &str) -> Result<Self, UploadError> {
        let http = reqwest::Client::builder()
            .timeout(config.timeout_duration())
            .build()
            .map_err(UploadError::Request)?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http,
        })
    }

    /// Upload a single observation. The token is resolved from the environment
    /// on each call. Returns the outcome on success.
    pub async fn upload(
        &self,
        observation: &ObservationV1,
        token: &str,
    ) -> Result<UploadOutcome, UploadError> {
        let body = serde_json::to_vec(observation)?;
        if body.len() > MAX_UNCOMPRESSED_JSON_BYTES {
            return Err(UploadError::PayloadTooLarge {
                len: body.len(),
                limit: MAX_UNCOMPRESSED_JSON_BYTES,
            });
        }

        let response = self
            .http
            .post(self.observations_url())
            .bearer_auth(token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .map_err(UploadError::Request)?;

        let status = response.status();
        match status {
            StatusCode::CREATED => Ok(UploadOutcome::Created),
            StatusCode::OK => Ok(UploadOutcome::Deduplicated),
            StatusCode::UNAUTHORIZED => Err(UploadError::Unauthorized),
            StatusCode::BAD_REQUEST => Err(UploadError::Rejected),
            StatusCode::CONFLICT => Err(UploadError::Conflict {
                event_id: observation.event_id,
            }),
            StatusCode::PAYLOAD_TOO_LARGE => Err(UploadError::ServerPayloadTooLarge),
            StatusCode::TOO_MANY_REQUESTS => Err(UploadError::RateLimited),
            other => Err(UploadError::UnexpectedStatus {
                status: other.as_u16(),
            }),
        }
    }

    fn observations_url(&self) -> String {
        format!("{}/{API_VERSION}/observations", self.base_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::config::UploadConfig;
    use crate::core::metrics::MetricSeriesSnapshot;
    use crate::core::models::{DiagnosisContext, DiagnosisResult, TargetMetadata};
    use crate::core::observations::v1::{ObservationBuildContext, build_observation};
    use chrono::Utc;
    use uuid::Uuid;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn observation() -> ObservationV1 {
        let result = DiagnosisResult::new(
            DiagnosisContext::new("5m").with_target(TargetMetadata {
                id: Some("target-1".to_string()),
                ..Default::default()
            }),
            MetricSeriesSnapshot::default(),
            vec![],
        );
        let context = ObservationBuildContext {
            event_id: Uuid::now_v7(),
            observed_at: Utc::now(),
            agent_id: "agent-1".to_string(),
            agent_version: "0.8.0".to_string(),
            local_rule_pack: "0.8.0".to_string(),
        };
        build_observation(&result, &context, 300).unwrap()
    }

    fn client_for(server: &MockServer) -> UploadClient {
        let config = UploadConfig {
            timeout: 5,
            enabled: true,
        };
        UploadClient::with_base_url(&config, &server.uri()).unwrap()
    }

    const TOKEN: &str = "test-token";

    #[test]
    fn production_endpoint_uses_major_version_path() {
        let client = UploadClient::new(&UploadConfig::default()).unwrap();
        assert_eq!(
            client.observations_url(),
            "https://api.vllm.doctor/v1/observations"
        );
    }

    #[tokio::test]
    async fn upload_201_created() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .and(header("authorization", format!("Bearer {TOKEN}")))
            .respond_with(ResponseTemplate::new(201))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let outcome = client.upload(&observation(), TOKEN).await.unwrap();
        assert_eq!(outcome, UploadOutcome::Created);
    }

    #[tokio::test]
    async fn upload_200_deduplicated() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let outcome = client.upload(&observation(), TOKEN).await.unwrap();
        assert_eq!(outcome, UploadOutcome::Deduplicated);
    }

    #[tokio::test]
    async fn upload_401_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.upload(&observation(), TOKEN).await.unwrap_err();
        assert!(matches!(err, UploadError::Unauthorized));
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn upload_409_conflict() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .respond_with(ResponseTemplate::new(409))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.upload(&observation(), TOKEN).await.unwrap_err();
        assert!(matches!(err, UploadError::Conflict { .. }));
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn upload_413_payload_too_large() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .respond_with(ResponseTemplate::new(413))
            .mount(&server)
            .await;

        let client = client_for(&server);
        let err = client.upload(&observation(), TOKEN).await.unwrap_err();
        assert!(matches!(err, UploadError::ServerPayloadTooLarge));
        assert!(!err.is_retryable());
    }

    #[tokio::test]
    async fn upload_400_rejected_is_not_retryable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let error = client_for(&server)
            .upload(&observation(), TOKEN)
            .await
            .unwrap_err();
        assert!(matches!(error, UploadError::Rejected));
        assert!(!error.is_retryable());
    }

    #[tokio::test]
    async fn upload_429_is_retryable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/observations"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let error = client_for(&server)
            .upload(&observation(), TOKEN)
            .await
            .unwrap_err();
        assert!(matches!(error, UploadError::RateLimited));
        assert!(error.is_retryable());
    }

    #[test]
    fn only_server_errors_are_retryable_unexpected_statuses() {
        assert!(UploadError::UnexpectedStatus { status: 500 }.is_retryable());
        assert!(UploadError::UnexpectedStatus { status: 503 }.is_retryable());
        assert!(!UploadError::UnexpectedStatus { status: 403 }.is_retryable());
    }

    #[tokio::test]
    async fn upload_network_error() {
        let config = UploadConfig {
            timeout: 2,
            enabled: true,
        };
        let client = UploadClient::with_base_url(&config, "http://127.0.0.1:1").unwrap();
        let err = client.upload(&observation(), TOKEN).await.unwrap_err();
        assert!(matches!(err, UploadError::Request(_)));
        assert!(err.is_retryable());
    }
}
