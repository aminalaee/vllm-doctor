//! Cloud upload transport and credentials.

pub mod client;
pub mod config;

pub use client::{UploadClient, UploadError, UploadOutcome};
pub use config::{TokenError, resolve_token};

use std::path::{Path, PathBuf};

use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentIdError {
    #[error("could not determine the home directory for the agent identity")]
    HomeDirectoryUnavailable,
    #[error("could not read agent identity from {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("agent identity file {0} is empty")]
    Empty(PathBuf),
    #[error("could not create agent identity directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not persist agent identity to {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

pub fn ensure_agent_id(agent_id: Option<&str>) -> Result<String, AgentIdError> {
    if let Some(id) = agent_id {
        return Ok(id.to_string());
    }

    let path = agent_id_path(dirs::home_dir())?;
    load_or_create_agent_id(&path)
}

fn agent_id_path(home_dir: Option<PathBuf>) -> Result<PathBuf, AgentIdError> {
    Ok(home_dir
        .ok_or(AgentIdError::HomeDirectoryUnavailable)?
        .join(".vllm-doctor")
        .join("agent-id"))
}

fn load_or_create_agent_id(path: &Path) -> Result<String, AgentIdError> {
    match std::fs::read_to_string(path) {
        Ok(existing) => return nonempty_agent_id(path, existing),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
        Err(source) => {
            return Err(AgentIdError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| AgentIdError::CreateDirectory {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let generated = Uuid::now_v7().to_string();
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            use std::io::Write;
            if let Err(source) = file.write_all(format!("{generated}\n").as_bytes()) {
                drop(file);
                let _ = std::fs::remove_file(path);
                return Err(AgentIdError::Write {
                    path: path.to_path_buf(),
                    source,
                });
            }
            Ok(generated)
        }
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = std::fs::read_to_string(path).map_err(|source| AgentIdError::Read {
                path: path.to_path_buf(),
                source,
            })?;
            nonempty_agent_id(path, existing)
        }
        Err(source) => Err(AgentIdError::Write {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn nonempty_agent_id(path: &Path, value: String) -> Result<String, AgentIdError> {
    let value = value.trim();
    if value.is_empty() {
        Err(AgentIdError::Empty(path.to_path_buf()))
    } else {
        Ok(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_configured_id() {
        assert_eq!(ensure_agent_id(Some("agent-abc")).unwrap(), "agent-abc");
    }

    #[test]
    fn missing_home_directory_is_rejected() {
        assert!(matches!(
            agent_id_path(None).unwrap_err(),
            AgentIdError::HomeDirectoryUnavailable
        ));
    }

    #[test]
    fn generates_and_reuses_persisted_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state").join("agent-id");

        let generated = load_or_create_agent_id(&path).unwrap();
        let reused = load_or_create_agent_id(&path).unwrap();

        assert_eq!(generated, reused);
        assert_eq!(std::fs::read_to_string(path).unwrap().trim(), generated);
    }

    #[test]
    fn rejects_empty_identity_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent-id");
        std::fs::write(&path, "\n").unwrap();

        assert!(matches!(
            load_or_create_agent_id(&path).unwrap_err(),
            AgentIdError::Empty(_)
        ));
    }
}
