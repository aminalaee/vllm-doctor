//! Cloud upload token resolution.
//!
//! The upload token is never a CLI argument and is never persisted to the
//! config file. It is resolved from the environment at call time so secrets
//! stay out of config files and command history.

use std::env;

use thiserror::Error;

/// Environment variable holding the upload bearer token.
pub const TOKEN_ENV: &str = "VLLM_DOCTOR_TOKEN";

/// Errors that can occur while resolving the upload token.
#[derive(Debug, Error)]
pub enum TokenError {
    /// `VLLM_DOCTOR_TOKEN` is not set.
    #[error("no upload token: set {TOKEN_ENV}")]
    Missing,
    /// The token environment variable is set but empty.
    #[error("upload token in {env} is empty")]
    Empty { env: &'static str },
}

/// Resolve the upload bearer token from the environment.
/// The token is trimmed of surrounding whitespace and never logged.
pub fn resolve_token() -> Result<String, TokenError> {
    if let Ok(value) = env::var(TOKEN_ENV) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(TokenError::Empty { env: TOKEN_ENV });
        }
        return Ok(trimmed.to_string());
    }

    Err(TokenError::Missing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _env = EnvGuard::set(TOKEN_ENV, Some("env-secret"));
        assert_eq!(resolve_token().unwrap(), "env-secret");
    }

    #[test]
    fn resolve_token_trims_whitespace() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _env = EnvGuard::set(TOKEN_ENV, Some("  spaced-secret  "));
        assert_eq!(resolve_token().unwrap(), "spaced-secret");
    }

    #[test]
    fn resolve_token_missing_when_not_set() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _env = EnvGuard::set(TOKEN_ENV, None);
        let err = resolve_token().unwrap_err();
        assert!(err.to_string().contains(TOKEN_ENV));
    }

    #[test]
    fn resolve_token_empty_env_is_error() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _env = EnvGuard::set(TOKEN_ENV, Some(""));
        assert!(matches!(
            resolve_token().unwrap_err(),
            TokenError::Empty { .. }
        ));
    }
}
