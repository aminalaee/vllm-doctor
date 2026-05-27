import httpx
import pytest

from vllm_doctor.clients import (
    PrometheusClient,
    PrometheusConnectionError,
    PrometheusError,
    PrometheusQueryError,
)
from vllm_doctor.clients.models import MetricSample

INSTANT_RESULT = [{"metric": {"__name__": "up"}, "value": [1234567890, "1"]}]
RANGE_RESULT = [
    {
        "metric": {"__name__": "up"},
        "values": [[1234567890, "1"], [1234567891, "2"]],
    }
]


def _transport(result: list[dict], result_type: str = "vector") -> httpx.MockTransport:
    body = {"status": "success", "data": {"resultType": result_type, "result": result}}
    return httpx.MockTransport(lambda _: httpx.Response(200, json=body))


def _error_transport(error: str) -> httpx.MockTransport:
    body = {"status": "error", "error": error}
    return httpx.MockTransport(lambda _: httpx.Response(200, json=body))


@pytest.fixture
def instant_client() -> PrometheusClient:
    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=_transport(INSTANT_RESULT)),
    )


@pytest.fixture
def empty_client() -> PrometheusClient:
    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=_transport([])),
    )


@pytest.fixture
def range_client() -> PrometheusClient:
    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=_transport(RANGE_RESULT, result_type="matrix")),
    )


class TestInstantQuery:
    async def test_returns_metric_samples(self, instant_client: PrometheusClient) -> None:
        result = await instant_client.query("up")
        assert len(result) == 1
        assert isinstance(result[0], MetricSample)
        assert result[0].value == 1.0

    async def test_empty_result(self, empty_client: PrometheusClient) -> None:
        assert await empty_client.query("up") == []

    async def test_passes_time_param(self) -> None:
        captured: list[httpx.Request] = []

        def handler(r: httpx.Request) -> httpx.Response:
            captured.append(r)
            body = {"status": "success", "data": {"resultType": "vector", "result": []}}
            return httpx.Response(200, json=body)

        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        )
        await client.query("up", time="2024-01-01T00:00:00Z")
        assert "time=2024-01-01T00%3A00%3A00Z" in str(captured[0].url)

    async def test_raises_on_api_error(self) -> None:
        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=_error_transport("bad query")),
        )
        with pytest.raises(PrometheusQueryError, match="bad query"):
            await client.query("invalid{")

    async def test_raises_on_http_error(self) -> None:
        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(500))),
        )
        with pytest.raises(PrometheusError):
            await client.query("up")

    async def test_raises_on_connection_error(self) -> None:
        def handler(_: httpx.Request) -> httpx.Response:
            raise httpx.ConnectError("refused")

        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        )
        with pytest.raises(PrometheusConnectionError):
            await client.query("up")


class TestRangeQuery:
    async def test_returns_metric_samples(self, range_client: PrometheusClient) -> None:
        result = await range_client.query_range("up", "now-1h", "now", "1m")
        assert len(result) == 2
        assert result[0].value == 1.0
        assert result[1].value == 2.0

    async def test_empty_result(self, empty_client: PrometheusClient) -> None:
        assert await empty_client.query_range("up", "now-1h", "now", "1m") == []

    async def test_raises_on_api_error(self) -> None:
        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=_error_transport("bad range query")),
        )
        with pytest.raises(PrometheusQueryError, match="bad range query"):
            await client.query_range("invalid{", "now-1h", "now", "1m")

    async def test_raises_on_connection_error(self) -> None:
        def handler(_: httpx.Request) -> httpx.Response:
            raise httpx.ConnectError("refused")

        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        )
        with pytest.raises(PrometheusConnectionError):
            await client.query_range("up", "now-1h", "now", "1m")

    async def test_raises_on_http_error(self) -> None:
        client = PrometheusClient(
            base_url="http://localhost:9090",
            client=httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(500))),
        )
        with pytest.raises(PrometheusError):
            await client.query_range("up", "now-1h", "now", "1m")


class TestContextManager:
    async def test_context_manager(self, empty_client: PrometheusClient) -> None:
        async with empty_client as client:
            assert await client.query("up") == []
