import httpx
import pytest
from typer.testing import CliRunner

from vllm_doctor.cli import app
from vllm_doctor.clients.scrape import ScrapeClient

SCRAPE_METRICS = """\
# TYPE vllm:num_requests_running gauge
vllm:num_requests_running{engine="0",model_name="Qwen/Qwen3-0.6B"} 0.0
# TYPE vllm:num_requests_waiting gauge
vllm:num_requests_waiting{engine="0",model_name="Qwen/Qwen3-0.6B"} 0.0
"""


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


@pytest.fixture
def scrape_client() -> ScrapeClient:
    transport = httpx.MockTransport(
        lambda _: httpx.Response(
            200, text=SCRAPE_METRICS, headers={"content-type": "text/plain"}
        )
    )
    return ScrapeClient(
        url="http://localhost:8000/metrics",
        client=httpx.AsyncClient(transport=transport),
    )


class TestCLI:
    def test_no_issues_detected(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        result = runner.invoke(app, ["--url", "http://localhost:8000/metrics"])
        assert result.exit_code == 0
        assert "No issues detected" in result.output

    def test_missing_url_exits_nonzero(self, runner: CliRunner) -> None:
        result = runner.invoke(app, [])
        assert result.exit_code != 0
