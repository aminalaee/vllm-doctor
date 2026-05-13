import asyncio

from vllm_doctor.models import MetricSnapshot
from vllm_doctor.prometheus import PrometheusClient
from vllm_doctor.promql import query_requests_running, query_requests_waiting


async def collect(
    client: PrometheusClient,
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
