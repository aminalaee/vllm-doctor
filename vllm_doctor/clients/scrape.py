import re

import httpx
from prometheus_client.parser import text_string_to_metric_families

from vllm_doctor.clients._http import _get
from vllm_doctor.clients.models import MetricSample


class ScrapeClient:
    """Reads raw Prometheus text format directly from a /metrics endpoint."""

    def __init__(self, url: str, timeout: float = 10.0, client: httpx.AsyncClient | None = None) -> None:
        self.url = url
        self._client = client or httpx.AsyncClient(timeout=timeout)

    async def query(self, metric_name: str) -> list[MetricSample]:
        response = await _get(self._client, self.url)
        return _parse(response.text, metric_name)

    async def query_increase(self, metric_name: str, window: str) -> None:
        return None

    async def query_percentile(
        self, metric: str, quantile: float, model: str | None = None, window: str = "5m"
    ) -> float | None:
        return None

    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "ScrapeClient":
        return self

    async def __aexit__(self, *args: object) -> None:
        await self.aclose()


def _parse(text: str, query: str) -> list[MetricSample]:
    if "{" in query:
        metric_name, label_str = query.split("{", 1)
        labels_filter: dict[str, str] = dict(re.findall(r'(\w+)="([^"]*)"', label_str))
    else:
        metric_name = query
        labels_filter = {}

    return [
        MetricSample(
            labels=s.labels,
            value=s.value,
            timestamp=float(s.timestamp) if s.timestamp is not None else None,
        )
        for family in text_string_to_metric_families(text)
        for s in family.samples
        if s.name == metric_name
        if all(s.labels.get(k) == v for k, v in labels_filter.items())
    ]
