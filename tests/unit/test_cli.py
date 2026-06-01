import json
from pathlib import Path

import httpx
import pytest
from freezegun import freeze_time
from typer.testing import CliRunner

from vllm_doctor.cli import app
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
        assert "OK" in result.output

    def test_live_zero_exits_nonzero(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["--url", "http://localhost:8000/metrics", "--live", "0"])
        assert result.exit_code != 0

    def test_live_negative_exits_nonzero(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["--url", "http://localhost:8000/metrics", "--live", "-1"])
        assert result.exit_code != 0

    def test_missing_url_exits_nonzero(self, runner: CliRunner) -> None:
        result = runner.invoke(app, [])
        assert result.exit_code != 0

    def test_live_interval_override(
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
        result = runner.invoke(app, ["--url", "http://localhost:8000/metrics", "--live", "10"])
        assert result.exit_code == 0
        assert "OK" in result.output

    def test_live_short_flag(
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
        result = runner.invoke(app, ["--url", "http://localhost:8000/metrics", "-l", "10"])
        assert result.exit_code == 0
        assert "OK" in result.output

    @freeze_time("2026-06-01 13:44:39 UTC")
    def test_json_format_contract(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.resolve_client", fake_resolve)
        result = runner.invoke(app, ["--url", "http://localhost:8000/metrics", "--format", "json"])
        assert result.exit_code == 0

        output = json.loads(result.output)

        assert output["schema_version"] == "1"
        assert output["metadata"]["generated_at"] == "2026-06-01T13:44:39+00:00"
        assert output["metadata"]["target"]["model_name"] is None
        assert output["metadata"]["target"]["window"] == "now"
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
