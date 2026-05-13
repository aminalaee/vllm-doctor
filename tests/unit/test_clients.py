import httpx

from vllm_doctor.clients import PrometheusClient, ScrapeClient, resolve_client

SAMPLE_METRICS = """\
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{model_name="llama"} 10.0
"""


class TestResolveClient:
    async def test_returns_scrape_client_for_metrics_endpoint(self) -> None:
        transport = httpx.MockTransport(
            lambda _: httpx.Response(
                200, text=SAMPLE_METRICS, headers={"content-type": "text/plain"}
            )
        )
        client = await resolve_client(
            "http://localhost:8000/metrics",
            client=httpx.AsyncClient(transport=transport),
        )
        assert isinstance(client, ScrapeClient)

    async def test_returns_prometheus_client_on_connect_error(self) -> None:
        def handler(_: httpx.Request) -> httpx.Response:
            raise httpx.ConnectError("refused")

        client = await resolve_client(
            "http://localhost:9090",
            client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        )
        assert isinstance(client, PrometheusClient)

    async def test_returns_prometheus_client_for_non_text_response(self) -> None:
        transport = httpx.MockTransport(
            lambda _: httpx.Response(
                200,
                json={"status": "ok"},
                headers={"content-type": "application/json"},
            )
        )
        client = await resolve_client(
            "http://localhost:9090",
            client=httpx.AsyncClient(transport=transport),
        )
        assert isinstance(client, PrometheusClient)
