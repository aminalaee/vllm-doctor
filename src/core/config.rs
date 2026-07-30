//! Core configuration: rule thresholds only.
//!
//! This module holds the threshold configuration consumed by the rule engine
//! and diagnosis orchestrator. It has no dependency on file loading (figment),
//! the database (sqlx), or any CLI concern. The CLI constructs a [`CoreConfig`]
//! from its own [`CliConfig`](crate::cli::config::CliConfig) at the call
//! boundary; the backend constructs one directly.
use serde::{Deserialize, Serialize};

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

/// Rule thresholds consumed by the diagnostic engine. This is the core
/// half of the configuration: the CLI maps its loaded [`CliConfig`] to this,
/// and the backend constructs one directly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoreConfig {
    pub rules: RulesConfig,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_are_stable() {
        let config = CoreConfig::default();
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
}
