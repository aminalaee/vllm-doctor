import httpx


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

    async def query(self, promql: str, time: str | None = None) -> list[dict]:
        params: dict[str, str] = {"query": promql}
        if time is not None:
            params["time"] = time

        try:
            response = await self._client.get(
                f"{self.base_url}/api/v1/query", params=params
            )
            response.raise_for_status()
        except httpx.ConnectError as e:
            raise PrometheusConnectionError(str(e)) from e
        except httpx.HTTPStatusError as e:
            raise PrometheusError(str(e)) from e

        data = response.json()
        if data.get("status") != "success":
            raise PrometheusQueryError(data.get("error", "unknown error"))

        return data["data"]["result"]

    async def query_range(
        self, promql: str, start: str, end: str, step: str
    ) -> list[dict]:
        params = {"query": promql, "start": start, "end": end, "step": step}

        try:
            response = await self._client.get(
                f"{self.base_url}/api/v1/query_range", params=params
            )
            response.raise_for_status()
        except httpx.ConnectError as e:
            raise PrometheusConnectionError(str(e)) from e
        except httpx.HTTPStatusError as e:
            raise PrometheusError(str(e)) from e

        data = response.json()
        if data.get("status") != "success":
            raise PrometheusQueryError(data.get("error", "unknown error"))

        return data["data"]["result"]

    async def aclose(self) -> None:
        await self._client.aclose()

    async def __aenter__(self) -> "PrometheusClient":
        return self

    async def __aexit__(self, *args: object) -> None:
        await self.aclose()
