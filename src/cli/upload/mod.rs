//! Cloud upload support for the vLLM-doctor CLI.
//!
//! This module owns the HTTP transport and token resolution for uploading
//! `ObservationV1` payloads to the vllm.doctor backend. The upload
//! configuration struct (`UploadConfig`) lives in [`crate::cli::config`]
//! alongside the rest of the CLI config model. Token resolution, the reqwest
//! client, and the upload error type live here. The observation contract
//! itself is defined in `core::observations::v1`.
pub mod client;
pub mod config;

pub use client::{UploadClient, UploadError, UploadOutcome};
pub use config::{TokenError, resolve_token};

use std::path::Path;

use uuid::Uuid;

/// Resolve a stable agent id for uploads.
///
/// If `config.agent.id` is set, it is returned as-is. Otherwise a fresh v7
/// UUID is generated and, when a writable config file path is known, persisted
/// back to that file so subsequent runs reuse the same identity. When no path
/// is known (e.g. config came from the default search locations), the
/// generated id is returned without persistence — it will differ on the next
/// run, but upload still works.
pub fn ensure_agent_id(agent_id: Option<&str>, config_path: Option<&Path>) -> String {
    if let Some(id) = agent_id {
        let trimmed = id.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let generated = Uuid::now_v7().to_string();
    if let Some(path) = config_path {
        if let Err(error) = persist_agent_id(path, &generated) {
            eprintln!("Warning: could not persist generated agent id: {error}");
        }
    }
    generated
}

/// Append or replace `[agent] id = "..."` in the config file at `path`.
/// Creates the file if it does not exist.
fn persist_agent_id(path: &Path, agent_id: &str) -> std::io::Result<()> {
    use std::io::Write;

    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let updated = replace_or_add_agent_id(&existing, agent_id);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    file.write_all(updated.as_bytes())?;
    Ok(())
}

/// Replace an existing `agent.id` line or append a new `[agent]` table.
fn replace_or_add_agent_id(existing: &str, agent_id: &str) -> String {
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();
    let mut in_agent_table = false;
    let mut replaced = false;

    for line in &mut lines {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_agent_table = trimmed == "[agent]";
            continue;
        }
        if in_agent_table && trimmed.starts_with("id") {
            if let Some(eq) = trimmed.find('=') {
                let prefix = &line[..eq + 1];
                *line = format!("{prefix} \"{agent_id}\"");
                replaced = true;
                break;
            }
        }
    }

    if !replaced {
        if !lines.is_empty() && !existing.ends_with('\n') {
            lines.push(String::new());
        }
        lines.push("[agent]".to_string());
        lines.push(format!("id = \"{agent_id}\""));
    }

    let mut result = lines.join("\n");
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_existing_id_when_present() {
        let id = ensure_agent_id(Some("agent-abc"), None);
        assert_eq!(id, "agent-abc");
    }

    #[test]
    fn ignores_whitespace_only_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.toml");
        let id = ensure_agent_id(Some("   "), Some(&path));
        assert!(!id.trim().is_empty());
        assert_ne!(id, "   ");
    }

    #[test]
    fn generates_and_persists_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.toml");
        std::fs::write(&path, "[database]\nurl = \"sqlite:///:memory:\"\n").unwrap();

        let id = ensure_agent_id(None, Some(&path));
        assert!(!id.is_empty());

        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("[agent]"));
        assert!(contents.contains(&id));
    }

    #[test]
    fn replaces_existing_id_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.toml");
        std::fs::write(
            &path,
            "[agent]\nlisten = \"127.0.0.1:9091\"\nid = \"old-id\"\n",
        )
        .unwrap();

        let id = ensure_agent_id(None, Some(&path));
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(!contents.contains("old-id"));
        assert!(contents.contains(&id));
        assert!(contents.contains("127.0.0.1:9091"));
    }

    #[test]
    fn persist_creates_file_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("cfg.toml");
        let id = ensure_agent_id(None, Some(&path));
        assert!(path.exists());
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains(&id));
    }

    #[test]
    fn replace_or_add_appends_when_no_agent_table() {
        let result = replace_or_add_agent_id("[database]\nurl = \"x\"\n", "new-id");
        assert!(result.contains("[agent]"));
        assert!(result.contains("id = \"new-id\""));
    }

    #[test]
    fn replace_or_add_handles_empty_input() {
        let result = replace_or_add_agent_id("", "new-id");
        assert!(result.contains("[agent]"));
        assert!(result.contains("id = \"new-id\""));
    }
}
