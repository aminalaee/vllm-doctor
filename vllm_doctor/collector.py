from abc import ABC, abstractmethod
from dataclasses import dataclass

from vllm_doctor.clients import Client
from vllm_doctor.models import Metrics
from vllm_doctor.probes import run_probes


class Metric(ABC):
    output: str

    @abstractmethod
    def probe_names(self) -> set[str]: ...

    @abstractmethod
    def compute(self, raw: dict[str, float | None]) -> float | None: ...


@dataclass(frozen=True)
class Direct(Metric):
    output: str
    probe: str

    def probe_names(self) -> set[str]:
        return {self.probe}

    def compute(self, raw: dict[str, float | None]) -> float | None:
        return raw[self.probe]


@dataclass(frozen=True)
class Ratio(Metric):
    output: str
    numerator: str
    denominator: str

    def probe_names(self) -> set[str]:
        return {self.numerator, self.denominator}

    def compute(self, raw: dict[str, float | None]) -> float | None:
        n, d = raw[self.numerator], raw[self.denominator]
        if n is None or d is None or d <= 0:
            return None
        return n / d


METRICS: list[Metric] = [
    Direct("num_requests_running", probe="num_requests_running"),
    Direct("num_requests_waiting", probe="num_requests_waiting"),
    Direct("kv_cache_usage_perc", probe="kv_cache_usage_perc"),
    Direct("prompt_tokens_per_second", probe="prompt_tokens_per_second"),
    Direct("generation_tokens_per_second", probe="generation_tokens_per_second"),
    Direct("request_success_total", probe="request_success_total"),
    Direct("request_error_total", probe="request_error_total"),
    Direct("request_abort_total", probe="request_abort_total"),
    Direct("ttft_p95_seconds", probe="ttft_p95_seconds"),
    Direct("tpot_p95_seconds", probe="tpot_p95_seconds"),
    Direct("queue_time_p95_seconds", probe="queue_time_p95_seconds"),
    Direct("num_preemptions_total", probe="num_preemptions_total"),
    Ratio("prefix_cache_hit_rate", numerator="prefix_hits", denominator="prefix_queries"),
]


async def collect(
    client: Client,
    since: str,
    model: str | None = None,
) -> Metrics:
    if since == "now":
        since = "5m"
    needed = {name for m in METRICS for name in m.probe_names()}
    raw = await run_probes(client, needed, since, model)
    return Metrics(**{m.output: m.compute(raw) for m in METRICS})
