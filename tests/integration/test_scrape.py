import httpx
import pytest

from vllm_doctor.clients import resolve_client, ScrapeClient
from vllm_doctor.collector import collect
from vllm_doctor.diagnosis import run
from vllm_doctor.models import DiagnosisResult, MetricSnapshot
from vllm_doctor.rules.error_rate import ErrorRateRule
from vllm_doctor.rules.kv_cache_pressure import KVCachePressureRule
from vllm_doctor.rules.low_throughput import LowThroughputRule
from vllm_doctor.rules.queue_pressure import QueuePressureRule
from vllm_doctor.rules.ttft_bottleneck import TTFTBottleneckRule

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
        assert snapshot.metrics.num_requests_running is not None
        assert snapshot.metrics.num_requests_waiting is not None
        assert snapshot.metrics.num_requests_running >= 0
        assert snapshot.metrics.num_requests_waiting >= 0

    async def test_diagnosis_runs_without_error(self, client: ScrapeClient) -> None:
        snapshot = await collect(client, window="now")
        findings = run(snapshot, [QueuePressureRule()])
        assert isinstance(findings, list)

    async def test_ttft_percentile_returns_none_in_scrape_mode(
        self, client: ScrapeClient
    ) -> None:
        result = await client.query_percentile("vllm:time_to_first_token_seconds", 0.95)
        assert result is None

    async def test_diagnosis_runs_with_all_rules(self, client: ScrapeClient) -> None:
        snapshot = await collect(client, window="now")
        all_rules = [
            QueuePressureRule(),
            KVCachePressureRule(),
            LowThroughputRule(),
            ErrorRateRule(),
            TTFTBottleneckRule(),
        ]
        result = DiagnosisResult(snapshot=snapshot, findings=run(snapshot, all_rules))
        assert isinstance(result.findings, list)

    async def test_raw_metrics_contain_vllm_prefix(self) -> None:
        r = httpx.get(METRICS_URL, timeout=5.0)
        assert "vllm:" in r.text
