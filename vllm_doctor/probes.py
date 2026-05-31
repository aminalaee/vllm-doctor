import asyncio
from collections.abc import Iterable
from dataclasses import dataclass, field
from enum import Enum

from vllm_doctor.clients import Client
from vllm_doctor.clients.models import label_selector

NUM_REQUESTS_RUNNING = "vllm:num_requests_running"
NUM_REQUESTS_WAITING = "vllm:num_requests_waiting"
GPU_CACHE_USAGE_PERC = "vllm:kv_cache_usage_perc"
REQUEST_SUCCESS_TOTAL = "vllm:request_success_total"
PROMPT_TOKENS_PER_SECOND = "vllm:prompt_tokens_per_second"
GENERATION_TOKENS_PER_SECOND = "vllm:generation_tokens_per_second"
TIME_TO_FIRST_TOKEN_SECONDS = "vllm:time_to_first_token_seconds"
TIME_PER_OUTPUT_TOKEN_SECONDS = "vllm:request_time_per_output_token_seconds"
PREFIX_CACHE_HITS_TOTAL = "vllm:prefix_cache_hits_total"
PREFIX_CACHE_QUERIES_TOTAL = "vllm:prefix_cache_queries_total"
REQUEST_QUEUE_TIME_SECONDS = "vllm:request_queue_time_seconds"
NUM_PREEMPTIONS_TOTAL = "vllm:num_preemptions_total"


class ProbeKind(Enum):
    gauge = "gauge"
    increase = "increase"
    percentile = "percentile"


@dataclass(frozen=True)
class Probe:
    kind: ProbeKind
    metric: str
    quantile: float = 0.0
    labels: dict[str, str] = field(default_factory=dict)


PROBES: dict[str, Probe] = {
    "num_requests_running": Probe(ProbeKind.gauge, NUM_REQUESTS_RUNNING),
    "num_requests_waiting": Probe(ProbeKind.gauge, NUM_REQUESTS_WAITING),
    "kv_cache_usage_perc": Probe(ProbeKind.gauge, GPU_CACHE_USAGE_PERC),
    "prompt_tokens_per_second": Probe(ProbeKind.gauge, PROMPT_TOKENS_PER_SECOND),
    "generation_tokens_per_second": Probe(ProbeKind.gauge, GENERATION_TOKENS_PER_SECOND),
    "request_success_total": Probe(ProbeKind.increase, REQUEST_SUCCESS_TOTAL, labels={"finished_reason": "stop"}),
    "request_error_total": Probe(ProbeKind.increase, REQUEST_SUCCESS_TOTAL, labels={"finished_reason": "error"}),
    "request_abort_total": Probe(ProbeKind.increase, REQUEST_SUCCESS_TOTAL, labels={"finished_reason": "abort"}),
    "ttft_p95_seconds": Probe(ProbeKind.percentile, TIME_TO_FIRST_TOKEN_SECONDS, quantile=0.95),
    "tpot_p95_seconds": Probe(ProbeKind.percentile, TIME_PER_OUTPUT_TOKEN_SECONDS, quantile=0.95),
    "queue_time_p95_seconds": Probe(ProbeKind.percentile, REQUEST_QUEUE_TIME_SECONDS, quantile=0.95),
    "num_preemptions_total": Probe(ProbeKind.increase, NUM_PREEMPTIONS_TOTAL),
    "prefix_hits": Probe(ProbeKind.increase, PREFIX_CACHE_HITS_TOTAL),
    "prefix_queries": Probe(ProbeKind.increase, PREFIX_CACHE_QUERIES_TOTAL),
}


async def _run(client: Client, probe: Probe, rate_window: str, model: str | None) -> float | None:
    expr = f"{probe.metric}{label_selector(model, **probe.labels)}"
    if probe.kind == ProbeKind.gauge:
        samples = await client.query(expr)
        return sum(s.value for s in samples) if samples else None
    if probe.kind == ProbeKind.increase:
        samples = await client.query_increase(expr, rate_window)
        if samples is None:
            samples = await client.query(expr)
        return sum(s.value for s in samples) if samples else None
    return await client.query_percentile(probe.metric, probe.quantile, model, rate_window)


async def run_probes(
    client: Client,
    names: Iterable[str],
    rate_window: str,
    model: str | None,
) -> dict[str, float | None]:
    names = list(names)
    values = await asyncio.gather(*[_run(client, PROBES[n], rate_window, model) for n in names])
    return dict(zip(names, values))
