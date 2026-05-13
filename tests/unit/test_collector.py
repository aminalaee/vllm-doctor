import httpx
import pytest

from vllm_doctor.collector import collect
from vllm_doctor.clients import PrometheusClient


def _make_client(running: str, waiting: str) -> PrometheusClient:
    call_count = 0

    def handler(r: httpx.Request) -> httpx.Response:
        nonlocal call_count
        if "num_requests_running" in str(r.url):
            value = running
        else:
            value = waiting
        result = [{"metric": {}, "value": [1234567890, value]}]
        body = {"status": "success", "data": {"resultType": "vector", "result": result}}
        call_count += 1
        return httpx.Response(200, json=body)

    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    )


@pytest.fixture
def client() -> PrometheusClient:
    return _make_client(running="10", waiting="3")


@pytest.fixture
def empty_client() -> PrometheusClient:
    body = {"status": "success", "data": {"resultType": "vector", "result": []}}
    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(
            transport=httpx.MockTransport(lambda _: httpx.Response(200, json=body))
        ),
    )


class TestCollect:
    async def test_returns_snapshot(self, client: PrometheusClient) -> None:
        snapshot = await collect(client, window="1h")
        assert snapshot.num_requests_running == 10.0
        assert snapshot.num_requests_waiting == 3.0
        assert snapshot.window == "1h"

    async def test_sets_model_name(self, client: PrometheusClient) -> None:
        snapshot = await collect(client, window="1h", model="meta-llama/Llama-3.1-8B")
        assert snapshot.model_name == "meta-llama/Llama-3.1-8B"

    async def test_missing_metrics_are_none(
        self, empty_client: PrometheusClient
    ) -> None:
        snapshot = await collect(empty_client, window="1h")
        assert snapshot.num_requests_running is None
        assert snapshot.num_requests_waiting is None
