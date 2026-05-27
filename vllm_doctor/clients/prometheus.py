import math

import httpx

from vllm_doctor.clients.models import MetricSample


class PrometheusError(Exception):
    pass


class PrometheusConnectionError(PrometheusError):
    pass


class PrometheusQueryError(PrometheusError):
    pass


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

        try:
            response = await self._client.get(f"{self.base_url}/api/v1/query", params=params)
            response.raise_for_status()
        except httpx.ConnectError as e:
            raise PrometheusConnectionError(str(e)) from e
        except httpx.HTTPStatusError as e:
            raise PrometheusError(str(e)) from e

        data = response.json()
        if data.get("status") != "success":
            raise PrometheusQueryError(data.get("error", "unknown error"))

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

        try:
            response = await self._client.get(f"{self.base_url}/api/v1/query_range", params=params)
            response.raise_for_status()
        except httpx.ConnectError as e:
            raise PrometheusConnectionError(str(e)) from e
        except httpx.HTTPStatusError as e:
            raise PrometheusError(str(e)) from e

        data = response.json()
        if data.get("status") != "success":
            raise PrometheusQueryError(data.get("error", "unknown error"))

        return [
            MetricSample(
                labels=r["metric"],
                value=float(point[1]),
                timestamp=float(point[0]),
            )
            for r in data["data"]["result"]
            for point in r["values"]
        ]

    async def query_percentile(
        self, metric: str, quantile: float, model: str | None = None, window: str = "5m"
    ) -> float | None:
        label = f'{{model_name="{model}"}}' if model else ""
        expr = f"histogram_quantile({quantile}, sum by (le) (rate({metric}_bucket{label}[{window}])))"
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
