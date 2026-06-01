import math

import httpx

from vllm_doctor.clients._http import _get
from vllm_doctor.clients.exceptions import ClientConnectionError, ClientError, ClientQueryError
from vllm_doctor.clients.models import MetricSample, label_selector

__all__ = ["PrometheusClient", "ClientConnectionError", "ClientError", "ClientQueryError"]


class PrometheusClient:
    def __init__(
        self,
        base_url: str,
        timeout: float = 10.0,
        client: httpx.AsyncClient | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self._client = client or httpx.AsyncClient(timeout=timeout)

    async def query(self, metric_name: str, time: str | None = None) -> list[MetricSample]:
        params: dict[str, str] = {"query": metric_name}
        if time is not None:
            params["time"] = time

        response = await _get(self._client, f"{self.base_url}/api/v1/query", params=params)
        data = response.json()
        if data.get("status") != "success":
            raise ClientQueryError(data.get("error", "unknown error"))

        return [
            MetricSample(
                labels=r["metric"],
                value=float(r["value"][1]),
                timestamp=float(r["value"][0]),
            )
            for r in data["data"]["result"]
        ]

    async def query_range(self, metric_name: str, start: str, end: str, step: str) -> list[MetricSample]:
        params = {"query": metric_name, "start": start, "end": end, "step": step}

        response = await _get(self._client, f"{self.base_url}/api/v1/query_range", params=params)
        data = response.json()
        if data.get("status") != "success":
            raise ClientQueryError(data.get("error", "unknown error"))

        return [
            MetricSample(
                labels=r["metric"],
                value=float(point[1]),
                timestamp=float(point[0]),
            )
            for r in data["data"]["result"]
            for point in r["values"]
        ]

    async def query_increase(self, metric_name: str, since: str) -> list[MetricSample]:
        return await self.query(f"increase({metric_name}[{since}])")

    async def query_percentile(
        self, metric: str, quantile: float, model: str | None = None, since: str = "5m"
    ) -> float | None:
        sel = label_selector(model)
        expr = f"histogram_quantile({quantile}, sum by (le) (rate({metric}_bucket{sel}[{since}])))"
        samples = await self.query(expr)
        if not samples or not math.isfinite(samples[0].value):
            return None
        return samples[0].value

    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "PrometheusClient":
        return self

    async def __aexit__(self, *args: object) -> None:
        await self.aclose()
