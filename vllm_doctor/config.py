from __future__ import annotations

import sys
from pathlib import Path

from pydantic import BaseModel, ConfigDict, Field

if sys.version_info >= (3, 11):
    import tomllib
else:
    import tomli as tomllib

_SEARCH_PATHS = [
    Path("vllm-doctor.toml"),
    Path.home() / ".config" / "vllm-doctor" / "config.toml",
]


class _StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class QueuePressureConfig(_StrictModel):
    high_waiting: int = 5
    high_running: int = 50


class QueueLatencyConfig(_StrictModel):
    high_queue_time_p95: float = 1.0


class KVCachePressureConfig(_StrictModel):
    high_cache_usage: float = 0.90


class PreemptionPressureConfig(_StrictModel):
    high_cache_usage: float = 0.80


class LowThroughputConfig(_StrictModel):
    low_prompt_tps: float = 10.0
    low_gen_tps: float = 50.0
    low_running: int = 2


class ErrorRateConfig(_StrictModel):
    high_error_rate: float = 0.05
    high_abort_rate: float = 0.10


class TTFTBottleneckConfig(_StrictModel):
    high_ttft_p95: float = 2.0
    high_tpot_p95: float = 0.2


class TPOTBottleneckConfig(_StrictModel):
    high_tpot_p95: float = 0.2
    low_gen_tokens_per_sec: float = 50.0


class PrefixCacheEfficiencyConfig(_StrictModel):
    min_hit_rate: float = 0.50


class ReplicaImbalanceConfig(_StrictModel):
    imbalance_factor: float = 2.0
    cache_gap: float = 0.30
    min_total_running: float = 5.0


def default_vllm_doctor_db_url() -> str:
    path = Path.home() / ".vllm-doctor" / "vllm_doctor.db"
    path.parent.mkdir(parents=True, exist_ok=True)
    return f"sqlite:///{path}"


class DatabaseConfig(_StrictModel):
    url: str = Field(default_factory=default_vllm_doctor_db_url)


class RulesConfig(_StrictModel):
    queue_pressure: QueuePressureConfig = QueuePressureConfig()
    queue_latency: QueueLatencyConfig = QueueLatencyConfig()
    kv_cache_pressure: KVCachePressureConfig = KVCachePressureConfig()
    preemption_pressure: PreemptionPressureConfig = PreemptionPressureConfig()
    low_throughput: LowThroughputConfig = LowThroughputConfig()
    error_rate: ErrorRateConfig = ErrorRateConfig()
    ttft_bottleneck: TTFTBottleneckConfig = TTFTBottleneckConfig()
    tpot_bottleneck: TPOTBottleneckConfig = TPOTBottleneckConfig()
    prefix_cache_efficiency: PrefixCacheEfficiencyConfig = PrefixCacheEfficiencyConfig()
    replica_imbalance: ReplicaImbalanceConfig = ReplicaImbalanceConfig()


class Config(_StrictModel):
    database: DatabaseConfig = Field(default_factory=DatabaseConfig)
    rules: RulesConfig = RulesConfig()


def load_config(path: Path | None = None) -> Config:
    if path is not None:
        with open(path, "rb") as f:
            return Config.model_validate(tomllib.load(f))

    for candidate in _SEARCH_PATHS:
        if candidate.exists():
            with open(candidate, "rb") as f:
                return Config.model_validate(tomllib.load(f))

    return Config()
