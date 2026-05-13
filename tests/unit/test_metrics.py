import httpx
import pytest

from vllm_doctor.clients import Client, PrometheusClient, ScrapeClient
from vllm_doctor.metrics import query_requests_running, query_requests_waiting

PROMETHEUS_BODY_SINGLE = {
    "status": "success",
    "data": {
        "resultType": "vector",
        "result": [{"metric": {"instance": "replica-0"}, "value": [1234567890, "4"]}],
    },
}
PROMETHEUS_BODY_MULTI = {
    "status": "success",
    "data": {
        "resultType": "vector",
        "result": [
            {"metric": {"instance": "replica-0"}, "value": [1234567890, "3"]},
            {"metric": {"instance": "replica-1"}, "value": [1234567890, "5"]},
            {"metric": {"instance": "replica-2"}, "value": [1234567890, "2"]},
        ],
    },
}
PROMETHEUS_BODY_EMPTY = {
    "status": "success",
    "data": {"resultType": "vector", "result": []},
}

SCRAPE_SINGLE = """\
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{instance="replica-0"} 4.0
# TYPE vllm:num_requests_waiting gauge
vllm:num_requests_waiting{instance="replica-0"} 4.0
"""
SCRAPE_MULTI = """\
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{instance="replica-0"} 3.0
vllm:num_requests_running{instance="replica-1"} 5.0
vllm:num_requests_running{instance="replica-2"} 2.0
# TYPE vllm:num_requests_waiting gauge
vllm:num_requests_waiting{instance="replica-0"} 3.0
vllm:num_requests_waiting{instance="replica-1"} 5.0
vllm:num_requests_waiting{instance="replica-2"} 2.0
"""
SCRAPE_EMPTY = ""


def _prometheus_client(body: dict) -> PrometheusClient:
    transport = httpx.MockTransport(lambda _: httpx.Response(200, json=body))
    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=transport),
    )


def _scrape_client(text: str) -> ScrapeClient:
    transport = httpx.MockTransport(
        lambda _: httpx.Response(200, text=text, headers={"content-type": "text/plain"})
    )
    return ScrapeClient(
        url="http://localhost:8000/metrics",
        client=httpx.AsyncClient(transport=transport),
    )


@pytest.fixture(params=["prometheus", "scrape"])
def client_with_value(request: pytest.FixtureRequest) -> Client:
    if request.param == "prometheus":
        return _prometheus_client(PROMETHEUS_BODY_SINGLE)
    return _scrape_client(SCRAPE_SINGLE)


@pytest.fixture(params=["prometheus", "scrape"])
def empty_client(request: pytest.FixtureRequest) -> Client:
    if request.param == "prometheus":
        return _prometheus_client(PROMETHEUS_BODY_EMPTY)
    return _scrape_client(SCRAPE_EMPTY)


@pytest.fixture(params=["prometheus", "scrape"])
def multi_replica_client(request: pytest.FixtureRequest) -> Client:
    if request.param == "prometheus":
        return _prometheus_client(PROMETHEUS_BODY_MULTI)
    return _scrape_client(SCRAPE_MULTI)


@pytest.fixture
def capturing_prometheus_client() -> tuple[PrometheusClient, list[httpx.Request]]:
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
    async def test_returns_value(self, client_with_value: Client) -> None:
        assert await query_requests_running(client_with_value) == 4.0

    async def test_sums_multiple_replicas(self, multi_replica_client: Client) -> None:
        assert await query_requests_running(multi_replica_client) == 10.0

    async def test_returns_none_when_empty(self, empty_client: Client) -> None:
        assert await query_requests_running(empty_client) is None

    async def test_filters_by_model(
        self, capturing_prometheus_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_prometheus_client
        await query_requests_running(client, model="meta-llama/Llama-3.1-8B")
        assert "meta-llama" in str(captured[0].url)


class TestQueryRequestsWaiting:
    async def test_returns_value(self, client_with_value: Client) -> None:
        assert await query_requests_waiting(client_with_value) == 4.0

    async def test_sums_multiple_replicas(self, multi_replica_client: Client) -> None:
        assert await query_requests_waiting(multi_replica_client) == 10.0

    async def test_returns_none_when_empty(self, empty_client: Client) -> None:
        assert await query_requests_waiting(empty_client) is None

    async def test_filters_by_model(
        self, capturing_prometheus_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_prometheus_client
        await query_requests_waiting(client, model="meta-llama/Llama-3.1-8B")
        assert "meta-llama" in str(captured[0].url)
