from vllm_doctor.config import RulesConfig
from vllm_doctor.rules.base import Rule
from vllm_doctor.rules.error_rate import ErrorRateRule
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.low_throughput import LowThroughputRule
from vllm_doctor.rules.preemption_pressure import PreemptionPressureRule
from vllm_doctor.rules.prefix_cache_efficiency import PrefixCacheEfficiencyRule
from vllm_doctor.rules.queue_latency import QueueLatencyRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule
from vllm_doctor.rules.replica_imbalance import ReplicaImbalanceRule
from vllm_doctor.rules.tpot_bottleneck import TPOTBottleneckRule
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule

_RULES: list[type[Rule]] = [
    QueuePressureRule,
    QueueLatencyRule,
    KVCachePressureRule,
    PreemptionPressureRule,
    LowThroughputRule,
    ErrorRateRule,
    TTFTBottleneckRule,
    TPOTBottleneckRule,
    PrefixCacheEfficiencyRule,
    ReplicaImbalanceRule,
]


def build_rules(config: RulesConfig) -> list[Rule]:
    return [cls.from_config(config) for cls in _RULES]
