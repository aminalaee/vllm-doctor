import json
from pathlib import Path

import httpx
import pytest
from freezegun import freeze_time
from typer.testing import CliRunner

from vllm_doctor import __version__
from vllm_doctor.cli import app
from vllm_doctor.cli.diagnose import _diagnose
from vllm_doctor.clients.scrape import ScrapeClient
from vllm_doctor.metrics import MetricSeriesSnapshot
from vllm_doctor.models import (
    ClientMode,
    Confidence,
    DiagnosisContext,
    DiagnosisResult,
    Finding,
    RuleResult,
    Severity,
)
from vllm_doctor.stores import HistoryStore

_HEALTHY_FIXTURE = (Path(__file__).parent.parent.parent / "fixtures" / "scrape" / "healthy.txt").read_text()


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


@pytest.fixture(autouse=True)
def isolated_home(tmp_path, monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setenv("HOME", str(tmp_path / "home"))


@pytest.fixture
def scrape_client() -> ScrapeClient:
    transport = httpx.MockTransport(
        lambda _: httpx.Response(200, text=_HEALTHY_FIXTURE, headers={"content-type": "text/plain"})
    )
    return ScrapeClient(
        url="http://localhost:8000/metrics",
        client=httpx.AsyncClient(transport=transport),
    )


class TestDiagnose:
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

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        result = runner.invoke(app, ["diagnose", "http://localhost:8000/metrics"])
        assert result.exit_code == 0
        assert "OK" in result.output

    def test_save_persists_one_shot_run(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        result = runner.invoke(app, ["diagnose", "http://localhost:8000/metrics", "--save", "--config", str(config)])

        assert result.exit_code == 0
        assert "OK" in result.output
        assert "Saved run:" in result.stderr
        run_id = result.stderr.strip().removeprefix("Saved run: ")

        with HistoryStore(f"sqlite:///{db}") as store:
            saved = store.get(run_id)

        assert saved is not None
        assert saved.context.client_mode.value == "scrape"

    def test_save_with_json_keeps_stdout_json(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        result = runner.invoke(
            app, ["diagnose", "http://localhost:8000/metrics", "--save", "--output", "json", "--config", str(config)]
        )

        assert result.exit_code == 0
        assert json.loads(result.stdout)["health"] == "ok"
        assert "Saved run:" in result.stderr

    def test_watch_saves_first_tick(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        async def fake_sleep(_: float) -> None:
            raise KeyboardInterrupt

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(
            app, ["diagnose", "http://localhost:8000/metrics", "--save", "--watch", "--config", str(config)]
        )

        assert result.exit_code == 0
        assert "Saved run:" in result.stderr
        assert "(initial)" in result.stderr

        run_id = result.stderr.strip().split("Saved run: ")[1].split()[0]
        with HistoryStore(f"sqlite:///{db}") as store:
            saved = store.get(run_id)
        assert saved is not None

    def test_watch_no_save_when_unchanged(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        call_count = 0

        async def fake_sleep(_: float) -> None:
            nonlocal call_count
            call_count += 1
            if call_count >= 2:
                raise KeyboardInterrupt

        diagnosis_result = DiagnosisResult(
            context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
            metric_series=MetricSeriesSnapshot(),
            checks=[],
        )

        async def fake_diagnose(*args: object, **kwargs: object) -> DiagnosisResult:
            return diagnosis_result

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose._diagnose", fake_diagnose)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(
            app, ["diagnose", "http://localhost:8000/metrics", "--save", "--watch", "--config", str(config)]
        )

        assert result.exit_code == 0
        # First tick saves; second identical tick does not
        assert result.stderr.count("Saved run:") == 1
        assert "(initial)" in result.stderr
        with HistoryStore(f"sqlite:///{db}") as store:
            runs = store.list()
        assert len(runs) == 1

    def test_watch_saves_on_health_transition(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        call_count = 0

        async def fake_sleep(_: float) -> None:
            nonlocal call_count
            call_count += 1
            if call_count >= 3:
                raise KeyboardInterrupt

        finding = Finding(
            severity=Severity.warning,
            confidence=Confidence.low,
            title="Queue pressure",
            summary="Requests are queuing.",
            evidence=["Waiting: 7"],
        )
        results = [
            DiagnosisResult(
                context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
                metric_series=MetricSeriesSnapshot(),
                checks=[],
            ),
            DiagnosisResult(
                context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
                metric_series=MetricSeriesSnapshot(),
                checks=[],
            ),
            DiagnosisResult(
                context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
                metric_series=MetricSeriesSnapshot(),
                checks=[RuleResult(id="queue_pressure", name="Queue Pressure", finding=finding)],
            ),
        ]
        it = iter(results)

        async def fake_diagnose(*args: object, **kwargs: object) -> DiagnosisResult:
            return next(it)

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose._diagnose", fake_diagnose)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(
            app, ["diagnose", "http://localhost:8000/metrics", "--save", "--watch", "--config", str(config)]
        )

        assert result.exit_code == 0
        # First tick + health transition = 2 saves
        assert result.stderr.count("Saved run:") == 2
        assert "(initial)" in result.stderr
        assert "→ warning" in result.stderr

        with HistoryStore(f"sqlite:///{db}") as store:
            runs = store.list()
        assert len(runs) == 2

    def test_watch_saves_on_rule_transition(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        call_count = 0

        async def fake_sleep(_: float) -> None:
            nonlocal call_count
            call_count += 1
            if call_count >= 3:
                raise KeyboardInterrupt

        finding_a = Finding(
            severity=Severity.warning,
            confidence=Confidence.low,
            title="Queue pressure",
            summary="Requests are queuing.",
            evidence=["Waiting: 7"],
        )
        finding_b = Finding(
            severity=Severity.warning,
            confidence=Confidence.low,
            title="KV cache pressure",
            summary="Cache is full.",
            evidence=["Usage: 95%"],
        )
        results = [
            DiagnosisResult(
                context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
                metric_series=MetricSeriesSnapshot(),
                checks=[RuleResult(id="queue_pressure", name="Queue Pressure", finding=finding_a)],
            ),
            DiagnosisResult(
                context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
                metric_series=MetricSeriesSnapshot(),
                checks=[RuleResult(id="queue_pressure", name="Queue Pressure", finding=finding_a)],
            ),
            DiagnosisResult(
                context=DiagnosisContext(since="now", client_mode=ClientMode.scrape),
                metric_series=MetricSeriesSnapshot(),
                checks=[RuleResult(id="kv_cache_pressure", name="KV Cache Pressure", finding=finding_b)],
            ),
        ]
        it = iter(results)

        async def fake_diagnose(*args: object, **kwargs: object) -> DiagnosisResult:
            return next(it)

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose._diagnose", fake_diagnose)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(
            app, ["diagnose", "http://localhost:8000/metrics", "--save", "--watch", "--config", str(config)]
        )

        assert result.exit_code == 0
        # First tick + rule transition = 2 saves
        assert result.stderr.count("Saved run:") == 2
        assert "(initial)" in result.stderr
        assert "(rules changed)" in result.stderr

        with HistoryStore(f"sqlite:///{db}") as store:
            runs = store.list()
        assert len(runs) == 2

    def test_watch_json_with_save(
        self,
        runner: CliRunner,
        scrape_client: ScrapeClient,
        monkeypatch: pytest.MonkeyPatch,
        tmp_path,
    ) -> None:
        async def fake_resolve(url: str, **_: object) -> ScrapeClient:
            return scrape_client

        async def fake_sleep(_: float) -> None:
            raise KeyboardInterrupt

        db = tmp_path / "history.db"
        config = tmp_path / "vllm-doctor.toml"
        config.write_text(f'[database]\nurl = "sqlite:///{db}"\n')

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(
            app,
            [
                "diagnose",
                "http://localhost:8000/metrics",
                "--save",
                "--watch",
                "--output",
                "json",
                "--config",
                str(config),
            ],
        )

        assert result.exit_code == 0
        lines = [line for line in result.stdout.splitlines() if line.strip()]
        assert len(lines) == 1
        assert json.loads(lines[0])["health"] == "ok"
        assert "Saved run:" in result.stderr

    def test_missing_url_exits_nonzero(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["diagnose"])
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

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        result = runner.invoke(app, ["diagnose", "http://localhost:8000/metrics"])
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

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        result = runner.invoke(
            app, ["diagnose", "http://localhost:8000/metrics", "--model", "llama-70b", "--output", "json"]
        )
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

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(app, ["diagnose", "http://localhost:8000/metrics", "--watch"])
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

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        monkeypatch.setattr("vllm_doctor.cli.diagnose.asyncio.sleep", fake_sleep)
        result = runner.invoke(app, ["diagnose", "http://localhost:8000/metrics", "-w"])
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

        monkeypatch.setattr("vllm_doctor.cli.diagnose.resolve_client", fake_resolve)
        result = runner.invoke(app, ["diagnose", "http://localhost:8000/metrics", "--output", "json"])
        assert result.exit_code == 0

        output = json.loads(result.output)

        assert output["schema_version"] == "1"
        assert output["metadata"]["generated_at"] == "2026-06-01T13:44:39+00:00"
        assert output["metadata"]["target"]["model_name"] is None
        assert output["metadata"]["target"]["since"] == "now"
        assert output["metadata"]["target"]["client_mode"] == "scrape"
        assert output["health"] == "ok"
        assert "notices" in output
        assert "checks" in output
        assert isinstance(output["checks"], list)

        for check in output["checks"]:
            assert "id" in check
            assert "name" in check
            assert "finding" in check
            assert isinstance(check["id"], str)
            assert len(check["id"]) > 0
