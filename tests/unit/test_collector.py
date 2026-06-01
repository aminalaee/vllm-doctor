import httpx
import pytest

from vllm_doctor.clients import PrometheusClient
from vllm_doctor.collector import collect


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
        client=httpx.AsyncClient(transport=httpx.MockTransport(lambda _: httpx.Response(200, json=body))),
    )


@pytest.fixture
def multi_replica_client() -> PrometheusClient:
    def handler(r: httpx.Request) -> httpx.Response:
        result = [
            {"metric": {"instance": "replica-0"}, "value": [1234567890, "3"]},
            {"metric": {"instance": "replica-1"}, "value": [1234567890, "5"]},
            {"metric": {"instance": "replica-2"}, "value": [1234567890, "2"]},
        ]
        body = {"status": "success", "data": {"resultType": "vector", "result": result}}
        return httpx.Response(200, json=body)

    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    )


@pytest.fixture
def capturing_client() -> tuple[PrometheusClient, list[httpx.Request]]:
    captured: list[httpx.Request] = []

    def handler(r: httpx.Request) -> httpx.Response:
        captured.append(r)
        body = {"status": "success", "data": {"resultType": "vector", "result": []}}
        return httpx.Response(200, json=body)

    return PrometheusClient(
        base_url="http://localhost:9090",
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    ), captured


class TestCollect:
    async def test_returns_metrics(self, client: PrometheusClient) -> None:
        metrics = await collect(client, since="1h")
        assert metrics.num_requests_running == 10.0
        assert metrics.num_requests_waiting == 3.0
        assert metrics.kv_cache_usage_perc == 0.72

    async def test_missing_metrics_are_none(self, empty_client: PrometheusClient) -> None:
        metrics = await collect(empty_client, since="1h")
        assert metrics.num_requests_running is None
        assert metrics.num_requests_waiting is None
        assert metrics.kv_cache_usage_perc is None

    async def test_sums_multiple_replicas(self, multi_replica_client: PrometheusClient) -> None:
        metrics = await collect(multi_replica_client, since="1h")
        assert metrics.num_requests_running == 10.0

    async def test_model_label_sent_in_query(
        self, capturing_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_client
        await collect(client, since="1h", model="meta-llama/Llama-3.1-8B")
        assert any("meta-llama" in str(r.url) for r in captured)

    async def test_prefix_hit_rate_computed(self, client: PrometheusClient) -> None:
        # client returns 0.72 for all metrics; hit_rate = 0.72 / 0.72 = 1.0
        metrics = await collect(client, since="1h")
        assert metrics.prefix_cache_hit_rate == 1.0

    async def test_prefix_hit_rate_none_when_no_queries(self, empty_client: PrometheusClient) -> None:
        metrics = await collect(empty_client, since="1h")
        assert metrics.prefix_cache_hit_rate is None

    async def test_counter_metrics_use_increase(
        self, capturing_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_client
        await collect(client, since="1h")
        queries = [r.url.params.get("query", "") for r in captured]
        assert any("increase(" in q and "num_preemptions_total" in q for q in queries)
        assert any("increase(" in q and "request_success_total" in q for q in queries)
        assert any("increase(" in q and "prefix_cache_hits_total" in q for q in queries)
        assert any("increase(" in q and "prefix_cache_queries_total" in q for q in queries)

    async def test_gauge_metrics_do_not_use_increase(
        self, capturing_client: tuple[PrometheusClient, list[httpx.Request]]
    ) -> None:
        client, captured = capturing_client
        await collect(client, since="1h")
        queries = [r.url.params.get("query", "") for r in captured]
        assert any("num_requests_running" in q and "increase(" not in q for q in queries)
        assert any("num_requests_waiting" in q and "increase(" not in q for q in queries)
        assert any("kv_cache_usage_perc" in q and "increase(" not in q for q in queries)
