import asyncio

from vllm_doctor.clients import Client
from vllm_doctor.metrics import (
    query_generation_tokens_per_second,
    query_gpu_cache_usage,
    query_prompt_tokens_per_second,
    query_requests_by_reason,
    query_requests_running,
    query_requests_waiting,
    query_tpot_p95,
    query_ttft_p95,
)
from vllm_doctor.models import Metrics, MetricSnapshot


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
    ) = await asyncio.gather(
        query_requests_running(client, model),
        query_requests_waiting(client, model),
        query_gpu_cache_usage(client, model),
        query_prompt_tokens_per_second(client, model),
        query_generation_tokens_per_second(client, model),
        query_requests_by_reason(client, "stop", model),
        query_requests_by_reason(client, "error", model),
        query_requests_by_reason(client, "abort", model),
        query_ttft_p95(client, model, rate_window),
        query_tpot_p95(client, model, rate_window),
    )
    return MetricSnapshot(
        model_name=model,
        window=window,
        metrics=Metrics(
            num_requests_running=running,
            num_requests_waiting=waiting,
            gpu_cache_usage_perc=gpu_cache,
            prompt_tokens_per_second=prompt_tps,
            generation_tokens_per_second=gen_tps,
            request_success_total=success,
            request_error_total=errors,
            request_abort_total=aborts,
            ttft_p95_seconds=ttft_p95,
            tpot_p95_seconds=tpot_p95,
        ),
    )
