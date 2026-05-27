from typing import Protocol, runtime_checkable

import httpx

from vllm_doctor.clients.models import MetricSample
from vllm_doctor.clients.prometheus import (
    PrometheusClient,
    PrometheusConnectionError,
    PrometheusError,
    PrometheusQueryError,
)
from vllm_doctor.clients.scrape import ScrapeClient


@runtime_checkable
class Client(Protocol):
    async def query(self, metric_name: str) -> list[MetricSample]: ...
    async def query_percentile(
        self, metric: str, quantile: float, model: str | None = None, window: str = "5m"
    ) -> float | None: ...
    async def aclose(self) -> None: ...


async def resolve_client(
    url: str,
    timeout: float = 10.0,
    client: httpx.AsyncClient | None = None,
) -> ScrapeClient | PrometheusClient:
    """Try direct scrape first, fall back to Prometheus mode."""
    http = client or httpx.AsyncClient(timeout=timeout)
    try:
        response = await http.get(url)
        if response.status_code == 200 and "text/plain" in response.headers.get("content-type", ""):
            return ScrapeClient(url, client=http)
    except httpx.ConnectError:
        pass
    return PrometheusClient(url, client=http)


__all__ = [
    "Client",
    "PrometheusClient",
    "PrometheusConnectionError",
    "PrometheusError",
    "PrometheusQueryError",
    "ScrapeClient",
    "resolve_client",
]
