import asyncio

from vllm_doctor.clients import Client
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
from vllm_doctor.models import Metrics, MetricSnapshot


def _metric_expr(metric: str, model: str | None, **labels: str) -> str:
    parts = [f'model_name="{model}"'] if model else []
    parts += [f'{k}="{v}"' for k, v in labels.items()]
    label = "{" + ",".join(parts) + "}" if parts else ""
    return f"{metric}{label}"


async def _query(client: Client, metric: str, model: str | None, **labels: str) -> float | None:
    samples = await client.query(_metric_expr(metric, model, **labels))
    return sum(s.value for s in samples) if samples else None


async def _query_increase(client: Client, metric: str, window: str, model: str | None, **labels: str) -> float | None:
    samples = await client.query_increase(_metric_expr(metric, model, **labels), window)
    if samples is None:
        return await _query(client, metric, model, **labels)
    return sum(s.value for s in samples) if samples else None


async def _query_percentile(
    client: Client, metric: str, quantile: float, model: str | None, window: str
) -> float | None:
    return await client.query_percentile(metric, quantile, model, window)


async def collect(
    client: Client,
    window: str,
    model: str | None = None,
) -> MetricSnapshot:
    rate_window = window if window != "now" else "5m"
    (
        running,
        waiting,
        gpu_cache,
        prompt_tps,
        gen_tps,
        success,
        errors,
        aborts,
        ttft_p95,
        tpot_p95,
        prefix_hits,
        prefix_queries,
        queue_time_p95,
        preemptions,
    ) = await asyncio.gather(
        _query(client, NUM_REQUESTS_RUNNING, model),
        _query(client, NUM_REQUESTS_WAITING, model),
        _query(client, GPU_CACHE_USAGE_PERC, model),
        _query(client, PROMPT_TOKENS_PER_SECOND, model),
        _query(client, GENERATION_TOKENS_PER_SECOND, model),
        _query_increase(client, REQUEST_SUCCESS_TOTAL, rate_window, model, finished_reason="stop"),
        _query_increase(client, REQUEST_SUCCESS_TOTAL, rate_window, model, finished_reason="error"),
        _query_increase(client, REQUEST_SUCCESS_TOTAL, rate_window, model, finished_reason="abort"),
        _query_percentile(client, TIME_TO_FIRST_TOKEN_SECONDS, 0.95, model, rate_window),
        _query_percentile(client, TIME_PER_OUTPUT_TOKEN_SECONDS, 0.95, model, rate_window),
        _query_increase(client, PREFIX_CACHE_HITS_TOTAL, rate_window, model),
        _query_increase(client, PREFIX_CACHE_QUERIES_TOTAL, rate_window, model),
        _query_percentile(client, REQUEST_QUEUE_TIME_SECONDS, 0.95, model, rate_window),
        _query_increase(client, NUM_PREEMPTIONS_TOTAL, rate_window, model),
    )
    prefix_hit_rate = (
        prefix_hits / prefix_queries
        if prefix_hits is not None and prefix_queries is not None and prefix_queries > 0
        else None
    )
    return MetricSnapshot(
        model_name=model,
        window=window,
        metrics=Metrics(
            num_requests_running=running,
            num_requests_waiting=waiting,
            kv_cache_usage_perc=gpu_cache,
            prompt_tokens_per_second=prompt_tps,
            generation_tokens_per_second=gen_tps,
            request_success_total=success,
            request_error_total=errors,
            request_abort_total=aborts,
            ttft_p95_seconds=ttft_p95,
            tpot_p95_seconds=tpot_p95,
            prefix_cache_hit_rate=prefix_hit_rate,
            queue_time_p95_seconds=queue_time_p95,
            num_preemptions_total=preemptions,
        ),
    )
