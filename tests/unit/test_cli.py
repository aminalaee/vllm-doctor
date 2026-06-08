import json
from pathlib import Path

import httpx
import pytest
from freezegun import freeze_time
from typer.testing import CliRunner

from vllm_doctor import __version__
from vllm_doctor.cli import _diagnose, app
from vllm_doctor.clients.scrape import ScrapeClient

_HEALTHY_FIXTURE = (Path(__file__).parent.parent / "fixtures" / "scrape" / "healthy.txt").read_text()


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


@pytest.fixture
def scrape_client() -> ScrapeClient:
    transport = httpx.MockTransport(
        lambda _: httpx.Response(200, text=_HEALTHY_FIXTURE, headers={"content-type": "text/plain"})
    )
    return ScrapeClient(
        url="http://localhost:8000/metrics",
        client=httpx.AsyncClient(transport=transport),
    )


class TestCLI:
    async def test_diagnosis_uses_metric_series(self, scrape_client: ScrapeClient) -> None:
        result = await _diagnose(scrape_client, rules=[], since="now")

        assert result.metric_series.num_requests_running.samples
        assert result.metrics.num_requests_running is not None

    def test_no_issues_detected(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        result = runner.invoke(app, ["http://localhost:8000/metrics"])
        assert result.exit_code == 0
        assert "OK" in result.output

    def test_missing_url_exits_nonzero(self, runner: CliRunner) -> None:
        result = runner.invoke(app, [])
        assert result.exit_code != 0

    def test_version_flag(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["--version"])
        assert result.exit_code == 0
        assert __version__ in result.output
        assert "vllm-doctor" in result.output

    def test_connection_error_exits_cleanly(
        self,
        runner: CliRunner,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        def handler(_: httpx.Request) -> httpx.Response:
            raise httpx.ConnectError("connection refused")

        unreachable = ScrapeClient(
            url="http://localhost:8000/metrics",
            client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        )

        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return unreachable

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        result = runner.invoke(app, ["http://localhost:8000/metrics"])
        assert result.exit_code == 1
        assert "could not read metrics" in result.output
        assert "Traceback" not in result.output

    def test_model_flag_sets_target(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        result = runner.invoke(app, ["http://localhost:8000/metrics", "--model", "llama-70b", "--output", "json"])
        assert result.exit_code == 0
        assert json.loads(result.output)["metadata"]["target"]["model_name"] == "llama-70b"

    def test_watch_loop_runs(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        async def fake_sleep(_: float) -> None:
            raise KeyboardInterrupt

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.asyncio.sleep", fake_sleep)
        result = runner.invoke(app, ["http://localhost:8000/metrics", "--watch"])
        assert result.exit_code == 0
        assert "OK" in result.output

    def test_watch_short_flag(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        async def fake_sleep(_: float) -> None:
            raise KeyboardInterrupt

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.asyncio.sleep", fake_sleep)
        result = runner.invoke(app, ["http://localhost:8000/metrics", "-w"])
        assert result.exit_code == 0
        assert "OK" in result.output

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_json_output_contract(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        result = runner.invoke(app, ["http://localhost:8000/metrics", "--output", "json"])
        assert result.exit_code == 0

        output = json.loads(result.output)

        assert output["schema_version"] == "1"
        assert output["metadata"]["generated_at"] == "2026-06-01T13:44:39+00:00"
        assert output["metadata"]["target"]["model_name"] is None
        assert output["metadata"]["target"]["since"] == "now"
        assert output["metadata"]["target"]["client_mode"] == "scrape"
        assert output["health"] == "ok"
        assert "notice" in output
        assert "checks" in output
        assert isinstance(output["checks"], list)

        for check in output["checks"]:
            assert "id" in check
            assert "name" in check
            assert "finding" in check
            assert isinstance(check["id"], str)
            assert len(check["id"]) > 0
