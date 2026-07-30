//! Cloud upload token resolution.
//!
//! The upload token is never a CLI argument and is never persisted to the
//! config file. It is resolved from the environment at call time so secrets
//! stay out of config files and command history.

use std::env;
use std::path::PathBuf;

use thiserror::Error;

/// Environment variable holding the upload bearer token.
pub const TOKEN_ENV: &str = "VLLM_DOCTOR_TOKEN";
/// Environment variable holding the path to a file with the upload bearer
/// token (Kubernetes secrets mount tokens as files).
pub const TOKEN_FILE_ENV: &str = "VLLM_DOCTOR_TOKEN_FILE";

/// Errors that can occur while resolving the upload token.
#[derive(Debug, Error)]
pub enum TokenError {
    /// Neither `VLLM_DOCTOR_TOKEN` nor `VLLM_DOCTOR_TOKEN_FILE` is set.
    #[error("no upload token: set {env} or {file_env}")]
    Missing {
        env: &'static str,
        file_env: &'static str,
    },
    /// The token environment variable is set but empty.
    #[error("upload token in {env} is empty")]
    Empty { env: &'static str },
    /// The token file path in `VLLM_DOCTOR_TOKEN_FILE` could not be read.
    #[error("could not read token file {path}: {source}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The token file was read but contained only whitespace.
    #[error("token file {path} is empty")]
    EmptyFile { path: PathBuf },
}

/// Resolve the upload bearer token from the environment.
///
/// `VLLM_DOCTOR_TOKEN` takes precedence over `VLLM_DOCTOR_TOKEN_FILE`. The
/// token is trimmed of surrounding whitespace and never logged.
pub fn resolve_token() -> Result<String, TokenError> {
    if let Ok(value) = env::var(TOKEN_ENV) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(TokenError::Empty { env: TOKEN_ENV });
        }
        return Ok(trimmed.to_string());
    }

    if let Ok(path) = env::var(TOKEN_FILE_ENV) {
        let path = PathBuf::from(path);
        let contents = std::fs::read_to_string(&path).map_err(|source| TokenError::ReadFile {
            path: path.clone(),
            source,
        })?;
        let trimmed = contents.trim();
        if trimmed.is_empty() {
            return Err(TokenError::EmptyFile { path });
        }
        return Ok(trimmed.to_string());
    }

    Err(TokenError::Missing {
        env: TOKEN_ENV,
        file_env: TOKEN_FILE_ENV,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard that saves and restores an environment variable.
    struct EnvGuard {
        key: &'static str,
        original: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: Option<&str>) -> Self {
            let original = env::var(key).ok();
            match value {
                Some(v) => unsafe { env::set_var(key, v) },
                None => unsafe { env::remove_var(key) },
            }
            Self { key, original }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.original {
                Some(v) => unsafe { env::set_var(self.key, v) },
                None => unsafe { env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn resolve_token_from_env() {
        let _env = EnvGuard::set(TOKEN_ENV, Some("env-secret"));
        let _file = EnvGuard::set(TOKEN_FILE_ENV, None);
        assert_eq!(resolve_token().unwrap(), "env-secret");
    }

    #[test]
    fn resolve_token_trims_whitespace() {
        let _env = EnvGuard::set(TOKEN_ENV, Some("  spaced-secret  "));
        let _file = EnvGuard::set(TOKEN_FILE_ENV, None);
        assert_eq!(resolve_token().unwrap(), "spaced-secret");
    }

    #[test]
    fn resolve_token_from_file() {
        let _env = EnvGuard::set(TOKEN_ENV, None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "file-secret\n").unwrap();
        let _file = EnvGuard::set(TOKEN_FILE_ENV, Some(path.to_str().unwrap()));
        assert_eq!(resolve_token().unwrap(), "file-secret");
    }

    #[test]
    fn resolve_token_env_takes_precedence_over_file() {
        let _env = EnvGuard::set(TOKEN_ENV, Some("env-secret"));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "file-secret").unwrap();
        let _file = EnvGuard::set(TOKEN_FILE_ENV, Some(path.to_str().unwrap()));
        assert_eq!(resolve_token().unwrap(), "env-secret");
    }

    #[test]
    fn resolve_token_missing_when_neither_set() {
        let _env = EnvGuard::set(TOKEN_ENV, None);
        let _file = EnvGuard::set(TOKEN_FILE_ENV, None);
        let err = resolve_token().unwrap_err();
        assert!(err.to_string().contains(TOKEN_ENV));
        assert!(err.to_string().contains(TOKEN_FILE_ENV));
    }

    #[test]
    fn resolve_token_empty_env_is_error() {
        let _env = EnvGuard::set(TOKEN_ENV, Some(""));
        let _file = EnvGuard::set(TOKEN_FILE_ENV, None);
        assert!(matches!(
            resolve_token().unwrap_err(),
            TokenError::Empty { .. }
        ));
    }

    #[test]
    fn resolve_token_missing_file_is_error() {
        let _env = EnvGuard::set(TOKEN_ENV, None);
        let _file = EnvGuard::set(TOKEN_FILE_ENV, Some("/nonexistent/vllm-doctor-token"));
        assert!(matches!(
            resolve_token().unwrap_err(),
            TokenError::ReadFile { .. }
        ));
    }

    #[test]
    fn resolve_token_empty_file_is_error() {
        let _env = EnvGuard::set(TOKEN_ENV, None);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty");
        std::fs::write(&path, "   \n").unwrap();
        let _file = EnvGuard::set(TOKEN_FILE_ENV, Some(path.to_str().unwrap()));
        assert!(matches!(
            resolve_token().unwrap_err(),
            TokenError::EmptyFile { .. }
        ));
    }
}
