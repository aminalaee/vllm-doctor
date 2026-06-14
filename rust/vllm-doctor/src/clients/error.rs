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
