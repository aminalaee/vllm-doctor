import httpx
import pytest

from vllm_doctor.prometheus import PrometheusClient
from vllm_doctor.promql import query_requests_running, query_requests_waiting


def _make_client(values: list[str] | None) -> PrometheusClient:
    if values is None:
        result = []
    else:
        result = [
            {"metric": {"instance": f"replica-{i}"}, "value": [1234567890, v]}
            for i, v in enumerate(values)
        ]
    body = {"status": "success", "data": {"resultType": "vector", "result": result}}
    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(
            transport=httpx.MockTransport(lambda _: httpx.Response(200, json=body))
        ),
    )


@pytest.fixture
def client_with_value() -> PrometheusClient:
    return _make_client(["4"])


@pytest.fixture
def empty_client() -> PrometheusClient:
    return _make_client(None)


@pytest.fixture
def multi_replica_client() -> PrometheusClient:
    return _make_client(["3", "5", "2"])


@pytest.fixture
def capturing_client() -> tuple[PrometheusClient, list[httpx.Request]]:
    captured: list[httpx.Request] = []

    def handler(r: httpx.Request) -> httpx.Response:
        captured.append(r)
        body = {"status": "success", "data": {"resultType": "vector", "result": []}}
        return httpx.Response(200, json=body)

    client = PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    )
    return client, captured


class TestQueryRequestsRunning:
    @pytest.mark.asyncio
    async def test_returns_value(self, client_with_value: PrometheusClient) -> None:
        assert await query_requests_running(client_with_value) == 4.0

    @pytest.mark.asyncio
    async def test_sums_multiple_replicas(
        self, multi_replica_client: PrometheusClient
    ) -> None:
        assert await query_requests_running(multi_replica_client) == 10.0

    @pytest.mark.asyncio
    async def test_returns_none_when_empty(
        self, empty_client: PrometheusClient
    ) -> None:
        assert await query_requests_running(empty_client) is None

    @pytest.mark.asyncio
    async def test_filters_by_model(
        self, capturing_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_client
        await query_requests_running(client, model="meta-llama/Llama-3.1-8B")
        assert "meta-llama" in str(captured[0].url)


class TestQueryRequestsWaiting:
    @pytest.mark.asyncio
    async def test_returns_value(self, client_with_value: PrometheusClient) -> None:
        assert await query_requests_waiting(client_with_value) == 4.0

    @pytest.mark.asyncio
    async def test_sums_multiple_replicas(
        self, multi_replica_client: PrometheusClient
    ) -> None:
        assert await query_requests_waiting(multi_replica_client) == 10.0

    @pytest.mark.asyncio
    async def test_returns_none_when_empty(
        self, empty_client: PrometheusClient
    ) -> None:
        assert await query_requests_waiting(empty_client) is None

    @pytest.mark.asyncio
    async def test_filters_by_model(
        self, capturing_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_client
        await query_requests_waiting(client, model="meta-llama/Llama-3.1-8B")
        assert "meta-llama" in str(captured[0].url)
