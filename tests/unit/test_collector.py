import httpx
import pytest

from vllm_doctor.collector import collect
from vllm_doctor.clients import PrometheusClient


@pytest.fixture
def client() -> PrometheusClient:
    def handler(r: httpx.Request) -> httpx.Response:
        url = str(r.url)
        if "num_requests_running" in url:
            value = "10"
        elif "num_requests_waiting" in url:
            value = "3"
        else:
            value = "0.72"
        result = [{"metric": {}, "value": [1234567890, value]}]
        body = {"status": "success", "data": {"resultType": "vector", "result": result}}
        return httpx.Response(200, json=body)

    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    )


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
        assert snapshot.metrics.num_requests_running == 10.0
        assert snapshot.metrics.num_requests_waiting == 3.0
        assert snapshot.metrics.kv_cache_usage_perc == 0.72
        assert snapshot.window == "1h"

    async def test_sets_model_name(self, client: PrometheusClient) -> None:
        snapshot = await collect(client, window="1h", model="meta-llama/Llama-3.1-8B")
        assert snapshot.model_name == "meta-llama/Llama-3.1-8B"

    async def test_missing_metrics_are_none(
        self, empty_client: PrometheusClient
    ) -> None:
        snapshot = await collect(empty_client, window="1h")
        assert snapshot.metrics.num_requests_running is None
        assert snapshot.metrics.num_requests_waiting is None
        assert snapshot.metrics.kv_cache_usage_perc is None
