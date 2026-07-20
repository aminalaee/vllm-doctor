//! Configuration model and loader.
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use figment::providers::{Format, Serialized, Toml};
use figment::{Error as FigmentError, Figment};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::models::InferenceEngine;

const LOCAL_CONFIG_FILE: &str = "vllm-doctor.toml";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("load error: {0}")]
    Load(#[from] Box<FigmentError>),
    #[error("not found: {0}")]
    NotFound(PathBuf),
    #[error("invalid target id: id is empty or whitespace-only")]
    InvalidTargetId,
}

fn config_dir_from(base: Option<PathBuf>) -> Option<PathBuf> {
    base.map(|d| d.join("vllm-doctor").join("config.toml"))
}

fn default_database_url_with_home(home: Option<PathBuf>) -> String {
    let mut path = home.unwrap_or_else(|| PathBuf::from("."));
    path.push(".vllm-doctor");
    path.push("vllm_doctor.db");
    format!("sqlite:///{}", path.display())
}

fn default_database_url() -> String {
    default_database_url_with_home(dirs::home_dir())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QueuePressureConfig {
    pub high_waiting: i64,
    pub high_running: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct QueueLatencyConfig {
    pub high_queue_time_p95: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KVCachePressureConfig {
    pub high_cache_usage: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PreemptionPressureConfig {
    pub high_cache_usage: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LowThroughputConfig {
    pub low_prompt_tps: f64,
    pub low_gen_tps: f64,
    pub low_running: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ErrorRateConfig {
    pub high_error_rate: f64,
    pub high_abort_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TtftBottleneckConfig {
    pub high_ttft_p95: f64,
    pub high_tpot_p95: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TpotBottleneckConfig {
    pub high_tpot_p95: f64,
    pub low_gen_tokens_per_sec: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrefixCacheEfficiencyConfig {
    pub min_hit_rate: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ReplicaImbalanceConfig {
    pub imbalance_factor: f64,
    pub cache_gap: f64,
    pub min_total_running: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseConfig {
    pub url: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: default_database_url(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RulesConfig {
    pub queue_pressure: QueuePressureConfig,
    pub queue_latency: QueueLatencyConfig,
    pub kv_cache_pressure: KVCachePressureConfig,
    pub preemption_pressure: PreemptionPressureConfig,
    pub low_throughput: LowThroughputConfig,
    pub error_rate: ErrorRateConfig,
    pub ttft_bottleneck: TtftBottleneckConfig,
    pub tpot_bottleneck: TpotBottleneckConfig,
    pub prefix_cache_efficiency: PrefixCacheEfficiencyConfig,
    pub replica_imbalance: ReplicaImbalanceConfig,
}

/// Operator-provided target identity and engine metadata.
///
/// All fields are optional except `engine`, which defaults to `vllm`. The CLI
/// does not generate or persist a target ID when absent — a later SaaS
/// enrollment change must require or generate one before upload.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TargetConfig {
    pub id: Option<String>,
    #[serde(default)]
    pub engine: InferenceEngine,
    pub engine_version: Option<String>,
    pub environment: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub listen: Option<SocketAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub database: DatabaseConfig,
    pub rules: RulesConfig,
    #[serde(default)]
    pub target: TargetConfig,
    #[serde(default)]
    pub agent: AgentConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            rules: RulesConfig {
                queue_pressure: QueuePressureConfig {
                    high_waiting: 5,
                    high_running: 50,
                },
                queue_latency: QueueLatencyConfig {
                    high_queue_time_p95: 1.0,
                },
                kv_cache_pressure: KVCachePressureConfig {
                    high_cache_usage: 0.90,
                },
                preemption_pressure: PreemptionPressureConfig {
                    high_cache_usage: 0.80,
                },
                low_throughput: LowThroughputConfig {
                    low_prompt_tps: 10.0,
                    low_gen_tps: 50.0,
                    low_running: 2,
                },
                error_rate: ErrorRateConfig {
                    high_error_rate: 0.05,
                    high_abort_rate: 0.10,
                },
                ttft_bottleneck: TtftBottleneckConfig {
                    high_ttft_p95: 2.0,
                    high_tpot_p95: 0.2,
                },
                tpot_bottleneck: TpotBottleneckConfig {
                    high_tpot_p95: 0.2,
                    low_gen_tokens_per_sec: 50.0,
                },
                prefix_cache_efficiency: PrefixCacheEfficiencyConfig { min_hit_rate: 0.50 },
                replica_imbalance: ReplicaImbalanceConfig {
                    imbalance_factor: 2.0,
                    cache_gap: 0.30,
                    min_total_running: 5.0,
                },
            },
            target: TargetConfig::default(),
            agent: AgentConfig::default(),
        }
    }
}

/// Load merged configuration, resolve the database default against the supplied
/// home directory, and validate target identity.
fn load_config_with(
    path: Option<&Path>,
    home_dir: Option<PathBuf>,
    config_dir: Option<PathBuf>,
) -> Result<Config, ConfigError> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));
    if let Some(p) = path {
        if !p.exists() {
            return Err(ConfigError::NotFound(p.to_path_buf()));
        }
        figment = figment.merge(Toml::file(p));
    } else {
        figment = figment.merge(Toml::file(LOCAL_CONFIG_FILE));
        if let Some(global) = config_dir_from(config_dir) {
            figment = figment.merge(Toml::file(global));
        }
    }
    let mut config: Config = figment.extract().map_err(Box::new)?;
    if config.database.url.is_empty() {
        config.database.url = default_database_url_with_home(home_dir);
    }
    if let Some(ref id) = config.target.id {
        if id.trim().is_empty() {
            return Err(ConfigError::InvalidTargetId);
        }
    }
    Ok(config)
}

pub fn load_config(path: Option<&Path>) -> Result<Config, ConfigError> {
    load_config_with(path, dirs::home_dir(), dirs::config_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_are_stable() {
        let config = Config::default();
        assert_eq!(config.rules.queue_pressure.high_waiting, 5);
        assert_eq!(config.rules.queue_pressure.high_running, 50);
        assert_eq!(config.rules.queue_latency.high_queue_time_p95, 1.0);
        assert_eq!(config.rules.kv_cache_pressure.high_cache_usage, 0.90);
        assert_eq!(config.rules.preemption_pressure.high_cache_usage, 0.80);
        assert_eq!(config.rules.low_throughput.low_prompt_tps, 10.0);
        assert_eq!(config.rules.low_throughput.low_gen_tps, 50.0);
        assert_eq!(config.rules.low_throughput.low_running, 2);
        assert_eq!(config.rules.error_rate.high_error_rate, 0.05);
        assert_eq!(config.rules.error_rate.high_abort_rate, 0.10);
        assert_eq!(config.rules.ttft_bottleneck.high_ttft_p95, 2.0);
        assert_eq!(config.rules.ttft_bottleneck.high_tpot_p95, 0.2);
        assert_eq!(config.rules.tpot_bottleneck.high_tpot_p95, 0.2);
        assert_eq!(config.rules.tpot_bottleneck.low_gen_tokens_per_sec, 50.0);
        assert_eq!(config.rules.prefix_cache_efficiency.min_hit_rate, 0.50);
        assert_eq!(config.rules.replica_imbalance.imbalance_factor, 2.0);
        assert_eq!(config.rules.replica_imbalance.cache_gap, 0.30);
        assert_eq!(config.rules.replica_imbalance.min_total_running, 5.0);
    }

    #[test]
    fn explicit_path_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cfg.toml");
        std::fs::write(
            &path,
            "[rules.kv_cache_pressure]\nhigh_cache_usage = 0.75\n",
        )
        .unwrap();
        let config = load_config_with(Some(&path), None, None).unwrap();
        assert_eq!(config.rules.kv_cache_pressure.high_cache_usage, 0.75);
        assert_eq!(config.rules.queue_pressure.high_waiting, 5);
    }

    #[test]
    fn missing_explicit_path_is_error() {
        let path = PathBuf::from("/tmp/definitely-missing-vllm-doctor.toml");
        let err = load_config_with(Some(&path), None, None).unwrap_err();
        assert!(err.to_string().contains("definitely-missing"));
    }

    #[test]
    fn malformed_explicit_path_is_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(
            &path,
            "[rules.queue_latency]\nhigh_queue_time_p95 = \"nope\"\n",
        )
        .unwrap();
        let err = load_config_with(Some(&path), None, None).unwrap_err();
        assert!(err.to_string().contains("load error"));
    }

    #[test]
    fn auto_discovers_local_toml() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("vllm-doctor.toml"),
            "[rules.queue_latency]\nhigh_queue_time_p95 = 2.5\n",
        )
        .unwrap();

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(dir.path()).unwrap();
        let config = load_config_with(None, None, None).unwrap();
        std::env::set_current_dir(original).unwrap();

        assert_eq!(config.rules.queue_latency.high_queue_time_p95, 2.5);
    }

    #[test]
    fn default_db_url_uses_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).expect("create home");
        let url = default_database_url_with_home(Some(home.clone()));
        assert!(url.starts_with("sqlite:///"));
        assert!(url.contains("/.vllm-doctor/vllm_doctor.db"));
    }

    #[test]
    fn full_config_fixture_loads() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("full-config.toml");
        let config = load_config(Some(&path)).unwrap();
        assert_eq!(config.database.url, "sqlite:///:memory:");
        assert_eq!(config.rules.queue_pressure.high_waiting, 10);
        assert_eq!(config.rules.queue_pressure.high_running, 100);
        assert_eq!(config.rules.queue_latency.high_queue_time_p95, 2.0);
        assert_eq!(config.rules.kv_cache_pressure.high_cache_usage, 0.85);
        assert_eq!(config.rules.preemption_pressure.high_cache_usage, 0.70);
        assert_eq!(config.rules.low_throughput.low_prompt_tps, 5.0);
        assert_eq!(config.rules.low_throughput.low_gen_tps, 20.0);
        assert_eq!(config.rules.low_throughput.low_running, 1);
        assert_eq!(config.rules.error_rate.high_error_rate, 0.10);
        assert_eq!(config.rules.error_rate.high_abort_rate, 0.20);
        assert_eq!(config.rules.ttft_bottleneck.high_ttft_p95, 3.0);
        assert_eq!(config.rules.ttft_bottleneck.high_tpot_p95, 0.3);
        assert_eq!(config.rules.tpot_bottleneck.high_tpot_p95, 0.3);
        assert_eq!(config.rules.tpot_bottleneck.low_gen_tokens_per_sec, 30.0);
        assert_eq!(config.rules.prefix_cache_efficiency.min_hit_rate, 0.70);
        assert_eq!(config.rules.replica_imbalance.imbalance_factor, 3.0);
        assert_eq!(config.rules.replica_imbalance.cache_gap, 0.30);
        assert_eq!(config.rules.replica_imbalance.min_total_running, 5.0);
    }

    #[test]
    fn global_config_dir_is_merged() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(config_dir.join("vllm-doctor")).unwrap();
        std::fs::write(
            config_dir.join("vllm-doctor").join("config.toml"),
            "[rules.low_throughput]\nlow_prompt_tps = 0.5\n",
        )
        .unwrap();

        let config = load_config_with(None, None, Some(config_dir)).unwrap();
        assert_eq!(config.rules.low_throughput.low_prompt_tps, 0.5);
    }

    #[test]
    fn empty_database_url_gets_default() {
        let home = tempfile::tempdir().unwrap();
        let _config = Config {
            database: DatabaseConfig { url: String::new() },
            ..Config::default()
        };
        let merged = load_config_with(None, Some(home.path().to_path_buf()), None).unwrap();
        assert!(!merged.database.url.is_empty());
    }

    #[test]
    fn config_dir_from_returns_none_for_none() {
        assert!(config_dir_from(None).is_none());
    }

    #[test]
    fn target_section_parses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target.toml");
        std::fs::write(
            &path,
            "[target]\nid = \"llama-serving-prod\"\nengine = \"vllm\"\nengine_version = \"0.8.0\"\nenvironment = \"production\"\n",
        )
        .unwrap();
        let config = load_config_with(Some(&path), None, None).unwrap();
        assert_eq!(config.target.id.as_deref(), Some("llama-serving-prod"));
        assert_eq!(config.target.engine, InferenceEngine::Vllm);
        assert_eq!(config.target.engine_version.as_deref(), Some("0.8.0"));
        assert_eq!(config.target.environment.as_deref(), Some("production"));
    }

    #[test]
    fn agent_listen_address_parses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.toml");
        std::fs::write(&path, "[agent]\nlisten = \"127.0.0.1:9091\"\n").unwrap();

        let config = load_config_with(Some(&path), None, None).unwrap();
        assert_eq!(config.agent.listen, Some("127.0.0.1:9091".parse().unwrap()));
    }

    #[test]
    fn invalid_agent_listen_address_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("agent.toml");
        std::fs::write(&path, "[agent]\nlisten = \"not-an-address\"\n").unwrap();
        assert!(load_config_with(Some(&path), None, None).is_err());
    }

    #[test]
    fn target_section_defaults_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty.toml");
        std::fs::write(&path, "[database]\nurl = \"sqlite:///:memory:\"\n").unwrap();
        let config = load_config_with(Some(&path), None, None).unwrap();
        assert_eq!(config.target.engine, InferenceEngine::Vllm);
        assert!(config.target.id.is_none());
        assert!(config.target.engine_version.is_none());
        assert!(config.target.environment.is_none());
    }

    #[test]
    fn unsupported_target_engine_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unsupported_engine.toml");
        std::fs::write(&path, "[target]\nengine = \"sglang\"\n").unwrap();
        assert!(load_config_with(Some(&path), None, None).is_err());
    }

    #[test]
    fn empty_target_id_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty_id.toml");
        std::fs::write(&path, "[target]\nid = \"\"\n").unwrap();
        let err = load_config_with(Some(&path), None, None).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidTargetId));
    }

    #[test]
    fn whitespace_target_id_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ws_id.toml");
        std::fs::write(&path, "[target]\nid = \"   \"\n").unwrap();
        let err = load_config_with(Some(&path), None, None).unwrap_err();
        assert!(matches!(err, ConfigError::InvalidTargetId));
    }
}
