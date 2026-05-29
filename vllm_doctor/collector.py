import asyncio
from dataclasses import dataclass, field
from enum import Enum

from vllm_doctor.clients import Client
from vllm_doctor.clients.models import label_selector
from vllm_doctor.metrics import (
    GENERATION_TOKENS_PER_SECOND,
    GPU_CACHE_USAGE_PERC,
    NUM_PREEMPTIONS_TOTAL,
    NUM_REQUESTS_RUNNING,
    NUM_REQUESTS_WAITING,
    PREFIX_CACHE_HITS_TOTAL,
    PREFIX_CACHE_QUERIES_TOTAL,
    PROMPT_TOKENS_PER_SECOND,
    REQUEST_QUEUE_TIME_SECONDS,
    REQUEST_SUCCESS_TOTAL,
    TIME_PER_OUTPUT_TOKEN_SECONDS,
    TIME_TO_FIRST_TOKEN_SECONDS,
)
from vllm_doctor.models import Metrics


class QueryKind(Enum):
    gauge = "gauge"
    increase = "increase"
    percentile = "percentile"


@dataclass
class MetricQuery:
    output: str
    kind: QueryKind
    metric: str
    quantile: float = 0.0
    labels: dict[str, str] = field(default_factory=dict)


_QUERIES: list[MetricQuery] = [
    MetricQuery("num_requests_running", QueryKind.gauge, NUM_REQUESTS_RUNNING),
    MetricQuery("num_requests_waiting", QueryKind.gauge, NUM_REQUESTS_WAITING),
    MetricQuery("kv_cache_usage_perc", QueryKind.gauge, GPU_CACHE_USAGE_PERC),
    MetricQuery("prompt_tokens_per_second", QueryKind.gauge, PROMPT_TOKENS_PER_SECOND),
    MetricQuery("generation_tokens_per_second", QueryKind.gauge, GENERATION_TOKENS_PER_SECOND),
    MetricQuery("request_success_total", QueryKind.increase, REQUEST_SUCCESS_TOTAL, labels={"finished_reason": "stop"}),
    MetricQuery("request_error_total", QueryKind.increase, REQUEST_SUCCESS_TOTAL, labels={"finished_reason": "error"}),
    MetricQuery("request_abort_total", QueryKind.increase, REQUEST_SUCCESS_TOTAL, labels={"finished_reason": "abort"}),
    MetricQuery("ttft_p95_seconds", QueryKind.percentile, TIME_TO_FIRST_TOKEN_SECONDS, quantile=0.95),
    MetricQuery("tpot_p95_seconds", QueryKind.percentile, TIME_PER_OUTPUT_TOKEN_SECONDS, quantile=0.95),
    MetricQuery("_prefix_hits", QueryKind.increase, PREFIX_CACHE_HITS_TOTAL),
    MetricQuery("_prefix_queries", QueryKind.increase, PREFIX_CACHE_QUERIES_TOTAL),
    MetricQuery("queue_time_p95_seconds", QueryKind.percentile, REQUEST_QUEUE_TIME_SECONDS, quantile=0.95),
    MetricQuery("num_preemptions_total", QueryKind.increase, NUM_PREEMPTIONS_TOTAL),
]


def _metric_expr(metric: str, model: str | None, **labels: str) -> str:
    return f"{metric}{label_selector(model, **labels)}"


async def _run_query(client: Client, q: MetricQuery, rate_window: str, model: str | None) -> float | None:
    expr = _metric_expr(q.metric, model, **q.labels)
    if q.kind == QueryKind.gauge:
        samples = await client.query(expr)
        return sum(s.value for s in samples) if samples else None
    if q.kind == QueryKind.increase:
        samples = await client.query_increase(expr, rate_window)
        if samples is None:
            samples = await client.query(expr)
        return sum(s.value for s in samples) if samples else None
    # percentile
    return await client.query_percentile(q.metric, q.quantile, model, rate_window)


async def collect(
    client: Client,
    window: str,
    model: str | None = None,
) -> Metrics:
    rate_window = window if window != "now" else "5m"

    values = await asyncio.gather(*[_run_query(client, q, rate_window, model) for q in _QUERIES])
    results: dict[str, float | None] = {q.output: v for q, v in zip(_QUERIES, values)}

    hits = results.pop("_prefix_hits")
    queries = results.pop("_prefix_queries")
    results["prefix_cache_hit_rate"] = (
        hits / queries if hits is not None and queries is not None and queries > 0 else None
    )

    return Metrics(**results)
