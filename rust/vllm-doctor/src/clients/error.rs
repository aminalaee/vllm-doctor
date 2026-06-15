//! Error types for the metrics clients.
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("connection error: {source}")]
    Connection { source: reqwest::Error },
    #[error("status error {status}: {source}")]
    Status { status: u16, source: reqwest::Error },
    #[error("query error: {0}")]
    Query(String),
    #[error("parse error: {0}")]
    Parse(String),
}

impl ClientError {
    pub fn is_connection(&self) -> bool {
        matches!(self, Self::Connection { .. })
    }

    pub fn is_timeout(&self) -> bool {
        matches!(
            self,
            Self::Connection { source } if source.is_timeout()
        )
    }
}

impl From<reqwest::Error> for ClientError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() || err.is_timeout() {
            Self::Connection { source: err }
        } else if let Some(status) = err.status() {
            Self::Status {
                status: status.as_u16(),
                source: err,
            }
        } else {
            Self::Connection { source: err }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_and_parse_errors_display() {
        assert_eq!(
            ClientError::Query("bad query".into()).to_string(),
            "query error: bad query"
        );
        assert_eq!(
            ClientError::Parse("bad line".into()).to_string(),
            "parse error: bad line"
        );
    }

    #[test]
    fn from_reqwest_maps_connection() {
        let err = reqwest::Client::new().get("http://[::1]:9/").send();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client_err: ClientError = rt.block_on(err).unwrap_err().into();
        assert!(client_err.is_connection());
    }

    #[test]
    fn is_timeout_false_for_query_error() {
        assert!(!ClientError::Query("x".into()).is_timeout());
    }
}
