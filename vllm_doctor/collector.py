import asyncio

from vllm_doctor.clients import Client
from vllm_doctor.metrics import query_requests_running, query_requests_waiting
from vllm_doctor.models import MetricSnapshot


async def collect(
    client: Client,
    window: str,
    model: str | None = None,
) -> MetricSnapshot:
    running, waiting = await asyncio.gather(
        query_requests_running(client, model),
        query_requests_waiting(client, model),
    )
    return MetricSnapshot(
        model_name=model,
        window=window,
        num_requests_running=running,
        num_requests_waiting=waiting,
    )
