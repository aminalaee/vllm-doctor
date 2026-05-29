import httpx
import pytest

from vllm_doctor.clients import (
    ClientConnectionError,
    ClientError,
    ScrapeClient,
)
from vllm_doctor.clients.scrape import _parse

SAMPLE_METRICS = """\
# HELP vllm:num_requests_waiting Number of requests waiting.
# TYPE vllm:num_requests_waiting gauge
vllm:num_requests_waiting{model_name="llama"} 3.0
# HELP vllm:num_requests_running Number of requests running.
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{model_name="llama"} 10.0
vllm:num_requests_running{model_name="llama",instance="replica-2"} 5.0
"""


@pytest.fixture
def scrape_client() -> ScrapeClient:
    transport = httpx.MockTransport(
        lambda _: httpx.Response(200, text=SAMPLE_METRICS, headers={"content-type": "text/plain"})
    )
    return ScrapeClient(
        url="http://localhost:8000/metrics",
        client=httpx.AsyncClient(transport=transport),
    )


@pytest.fixture
def error_client() -> ScrapeClient:
    transport = httpx.MockTransport(lambda _: httpx.Response(500))
    return ScrapeClient(
        url="http://localhost:8000/metrics",
        client=httpx.AsyncClient(transport=transport),
    )


class TestParse:
    def test_parses_single_series(self) -> None:
        result = _parse(SAMPLE_METRICS, "vllm:num_requests_waiting")
        assert len(result) == 1
        assert result[0].value == 3.0

    def test_parses_multiple_series(self) -> None:
        result = _parse(SAMPLE_METRICS, "vllm:num_requests_running")
        assert len(result) == 2

    def test_returns_empty_for_missing_metric(self) -> None:
        assert _parse(SAMPLE_METRICS, "vllm:nonexistent") == []

    def test_includes_labels(self) -> None:
        result = _parse(SAMPLE_METRICS, "vllm:num_requests_waiting")
        assert result[0].labels["model_name"] == "llama"

    def test_filters_by_label(self) -> None:
        result = _parse(
            SAMPLE_METRICS,
            'vllm:num_requests_running{model_name="llama",instance="replica-2"}',
        )
        assert len(result) == 1
        assert result[0].value == 5.0


class TestScrapeClient:
    async def test_query_returns_result(self, scrape_client: ScrapeClient) -> None:
        result = await scrape_client.query("vllm:num_requests_waiting")
        assert len(result) == 1
        assert result[0].value == 3.0

    async def test_query_empty_for_missing_metric(self, scrape_client: ScrapeClient) -> None:
        result = await scrape_client.query("vllm:nonexistent")
        assert result == []

    async def test_raises_on_http_error(self, error_client: ScrapeClient) -> None:
        with pytest.raises(ClientError):
            await error_client.query("vllm:num_requests_waiting")

    async def test_raises_on_connection_error(self) -> None:
        def handler(_: httpx.Request) -> httpx.Response:
            raise httpx.ConnectError("refused")

        client = ScrapeClient(
            url="http://localhost:8000/metrics",
            client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        )
        with pytest.raises(ClientConnectionError):
            await client.query("vllm:num_requests_waiting")

    async def test_context_manager(self, scrape_client: ScrapeClient) -> None:
        async with scrape_client as c:
            result = await c.query("vllm:num_requests_waiting")
            assert len(result) == 1

    async def test_query_percentile_returns_none(self, scrape_client: ScrapeClient) -> None:
        result = await scrape_client.query_percentile("vllm:time_to_first_token_seconds", 0.95)
        assert result is None

    async def test_query_increase_returns_none(self, scrape_client: ScrapeClient) -> None:
        result = await scrape_client.query_increase("vllm:num_preemptions_total", "5m")
        assert result is None
