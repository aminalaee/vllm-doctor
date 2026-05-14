import httpx
import pytest

from vllm_doctor.clients import resolve_client, ScrapeClient
from vllm_doctor.collector import collect
from vllm_doctor.diagnosis import run
from vllm_doctor.models import MetricSnapshot
from vllm_doctor.rules.queue_pressure import QueuePressureRule

METRICS_URL = "http://localhost:8000/metrics"


@pytest.fixture
async def client() -> ScrapeClient:
    c = await resolve_client(METRICS_URL)
    assert isinstance(c, ScrapeClient), "Expected ScrapeClient for /metrics endpoint"
    return c


class TestLiveScrape:
    async def test_resolve_returns_scrape_client(self) -> None:
        c = await resolve_client(METRICS_URL)
        assert isinstance(c, ScrapeClient)
        await c.aclose()

    async def test_requests_running_is_numeric(self, client: ScrapeClient) -> None:
        samples = await client.query("vllm:num_requests_running")
        assert len(samples) >= 1
        assert isinstance(samples[0].value, float)

    async def test_requests_waiting_is_numeric(self, client: ScrapeClient) -> None:
        samples = await client.query("vllm:num_requests_waiting")
        assert len(samples) >= 1
        assert isinstance(samples[0].value, float)

    async def test_snapshot_fields_populated(self, client: ScrapeClient) -> None:
        snapshot = await collect(client, window="now")
        assert isinstance(snapshot, MetricSnapshot)
        assert snapshot.num_requests_running is not None
        assert snapshot.num_requests_waiting is not None
        assert snapshot.num_requests_running >= 0
        assert snapshot.num_requests_waiting >= 0

    async def test_diagnosis_runs_without_error(self, client: ScrapeClient) -> None:
        snapshot = await collect(client, window="now")
        findings = run(snapshot, [QueuePressureRule()])
        assert isinstance(findings, list)

    async def test_raw_metrics_contain_vllm_prefix(self) -> None:
        r = httpx.get(METRICS_URL, timeout=5.0)
        assert "vllm:" in r.text
