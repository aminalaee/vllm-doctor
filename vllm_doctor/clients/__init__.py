from typing import Protocol, runtime_checkable

import httpx

from vllm_doctor.clients.exceptions import ClientConnectionError, ClientError, ClientQueryError
from vllm_doctor.clients.models import MetricSample
from vllm_doctor.clients.prometheus import PrometheusClient
from vllm_doctor.clients.scrape import ScrapeClient


@runtime_checkable
class Client(Protocol):
    async def query(self, metric_name: str) -> list[MetricSample]: ...
    async def query_increase(self, metric_name: str, since: str) -> list[MetricSample] | None: ...
    async def query_percentile(
        self, metric: str, quantile: float, model: str | None = None, since: str = "5m"
    ) -> float | None: ...
    async def aclose(self) -> None: ...


_SCRAPE_CONTENT_TYPES = ("text/plain", "application/openmetrics-text")


async def resolve_client(
    url: str,
    timeout: float = 10.0,
    client: httpx.AsyncClient | None = None,
) -> ScrapeClient | PrometheusClient:
    """Try direct scrape first, fall back to Prometheus mode."""
    http = client or httpx.AsyncClient(timeout=timeout)
    try:
        response = await http.get(url)
        content_type = response.headers.get("content-type", "")
        if response.status_code == 200 and any(ct in content_type for ct in _SCRAPE_CONTENT_TYPES):
            return ScrapeClient(url, client=http)
    except httpx.HTTPError:
        pass
    return PrometheusClient(url, client=http)


__all__ = [
    "Client",
    "PrometheusClient",
    "ClientConnectionError",
    "ClientError",
    "ClientQueryError",
    "ScrapeClient",
    "resolve_client",
]
