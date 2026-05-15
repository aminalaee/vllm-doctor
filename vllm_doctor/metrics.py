from vllm_doctor.clients import Client
from vllm_doctor.clients.models import MetricSample


def _sum_values(samples: list[MetricSample]) -> float | None:
    if not samples:
        return None
    return sum(s.value for s in samples)


async def query_requests_running(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:num_requests_running{label}"))


async def query_requests_waiting(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:num_requests_waiting{label}"))


async def query_gpu_cache_usage(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:gpu_cache_usage_perc{label}"))


async def query_prompt_tokens_per_second(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:prompt_tokens_per_second{label}"))


async def query_generation_tokens_per_second(
    client: Client, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:generation_tokens_per_second{label}"))
