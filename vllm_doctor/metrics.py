from vllm_doctor.clients import Client
from vllm_doctor.clients.models import MetricSample

NUM_REQUESTS_RUNNING = "vllm:num_requests_running"
NUM_REQUESTS_WAITING = "vllm:num_requests_waiting"
GPU_CACHE_USAGE_PERC = "vllm:kv_cache_usage_perc"
REQUEST_SUCCESS_TOTAL = "vllm:request_success_total"
PROMPT_TOKENS_PER_SECOND = "vllm:prompt_tokens_per_second"
GENERATION_TOKENS_PER_SECOND = "vllm:generation_tokens_per_second"
TIME_TO_FIRST_TOKEN_SECONDS = "vllm:time_to_first_token_seconds"
TIME_PER_OUTPUT_TOKEN_SECONDS = "vllm:request_time_per_output_token_seconds"


def _sum_values(samples: list[MetricSample]) -> float | None:
    if not samples:
        return None
    return sum(s.value for s in samples)


async def query_requests_running(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"{NUM_REQUESTS_RUNNING}{label}"))


async def query_requests_waiting(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"{NUM_REQUESTS_WAITING}{label}"))


async def query_gpu_cache_usage(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"{GPU_CACHE_USAGE_PERC}{label}"))


async def query_requests_by_reason(
    client: Client, reason: str, model: str | None = None
) -> float | None:
    model_label = f'model_name="{model}",' if model else ""
    return _sum_values(
        await client.query(
            f'{REQUEST_SUCCESS_TOTAL}{{{model_label}finished_reason="{reason}"}}'
        )
    )


async def query_prompt_tokens_per_second(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"{PROMPT_TOKENS_PER_SECOND}{label}"))


async def query_generation_tokens_per_second(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"{GENERATION_TOKENS_PER_SECOND}{label}"))


async def query_ttft_p95(
    client: Client, model: str | None = None, window: str = "5m"
) -> float | None:
    return await client.query_percentile(
        TIME_TO_FIRST_TOKEN_SECONDS, 0.95, model, window
    )


async def query_tpot_p95(
    client: Client, model: str | None = None, window: str = "5m"
) -> float | None:
    return await client.query_percentile(
        TIME_PER_OUTPUT_TOKEN_SECONDS, 0.95, model, window
    )
