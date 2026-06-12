import json
from collections.abc import Generator
from pathlib import Path

import pytest
from typer.testing import CliRunner

from vllm_doctor.cli import app
from vllm_doctor.clients.models import MetricSample
from vllm_doctor.config import Config, DatabaseConfig
from vllm_doctor.metrics import MetricSeries, MetricSeriesSnapshot
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
from vllm_doctor.stores.migrate import run_migrations


@pytest.fixture(autouse=True)
def config(monkeypatch: pytest.MonkeyPatch, db_url: str) -> Generator[Config, None, None]:
    cfg = Config(database=DatabaseConfig(url=db_url))

    def _load_config(_path: Path | None = None) -> Config:
        return cfg

    monkeypatch.setattr("vllm_doctor.cli.history.load_config", _load_config)
    monkeypatch.setattr("vllm_doctor.cli.diagnose.load_config", _load_config)
    return cfg


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


@pytest.fixture
def saved_run(db_url: str) -> str:
    finding = Finding(
        severity=Severity.warning,
        confidence=Confidence.low,
        title="Queue pressure",
        summary="Requests are queuing faster than the server can process them.",
        evidence=["Waiting requests: 7"],
    )
    result = DiagnosisResult(
        context=DiagnosisContext(since="1h", model_name="meta-llama/Llama-3.1-8B", client_mode=ClientMode.prometheus),
        metric_series=MetricSeriesSnapshot(
            num_requests_running=MetricSeries(samples=[MetricSample(labels={}, value=5.0)]),
            num_requests_waiting=MetricSeries(samples=[MetricSample(labels={}, value=7.0)]),
        ),
        checks=[
            RuleResult(id="queue_pressure", name="Queue Pressure", finding=finding),
            RuleResult(id="kv_cache_pressure", name="KV Cache Pressure"),
        ],
    )
    run_migrations(db_url)
    with HistoryStore(db_url) as store:
        return store.save(result)


class TestHistoryList:
    def test_list_empty(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["history", "list"])
        assert result.exit_code == 0
        assert "No saved diagnosis runs found." in result.output

    def test_list_with_runs(self, runner: CliRunner, saved_run: str) -> None:
        result = runner.invoke(app, ["history", "list"], env={"COLUMNS": "120"})
        assert result.exit_code == 0
        # Rich truncates in narrow columns; assert on visible prefixes
        assert saved_run[:24] in result.output
        assert "meta-llama" in result.output
        assert "warn" in result.output
        assert "1" in result.output
        # Mode is hidden in non-verbose
        assert "promet" not in result.output

    def test_list_verbose(self, runner: CliRunner, saved_run: str) -> None:
        result = runner.invoke(app, ["history", "list", "--verbose"], env={"COLUMNS": "120"})
        assert result.exit_code == 0
        assert saved_run[:24] in result.output
        assert "meta-llama" in result.output
        assert "promet" in result.output
        assert "warn" in result.output
        assert "1" in result.output

    def test_list_json_empty(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["history", "list", "--output", "json"])
        assert result.exit_code == 0
        assert json.loads(result.output) == []

    def test_list_json(self, runner: CliRunner, saved_run: str) -> None:
        result = runner.invoke(app, ["history", "list", "--output", "json"])
        assert result.exit_code == 0
        data = json.loads(result.output)
        assert len(data) == 1
        assert data[0]["run_id"] == saved_run
        assert data[0]["model_name"] == "meta-llama/Llama-3.1-8B"
        assert data[0]["health"] == "warning"
        assert data[0]["fired_count"] == 1


class TestHistoryShow:
    def test_show_found(self, runner: CliRunner, saved_run: str) -> None:
        result = runner.invoke(app, ["history", "show", saved_run])
        assert result.exit_code == 0
        assert "Queue pressure" in result.output
        assert "Waiting requests: 7" in result.output

    def test_show_not_found(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["history", "show", "does-not-exist"])
        assert result.exit_code == 1
        assert "not found" in result.output

    def test_show_json(self, runner: CliRunner, saved_run: str) -> None:
        result = runner.invoke(app, ["history", "show", saved_run, "--output", "json"])
        assert result.exit_code == 0
        data = json.loads(result.output)
        assert data["schema_version"] == "1"
        assert data["health"] == "warning"
        assert data["metadata"]["target"]["model_name"] == "meta-llama/Llama-3.1-8B"

    def test_show_verbose(self, runner: CliRunner, saved_run: str) -> None:
        result = runner.invoke(app, ["history", "show", saved_run, "--verbose"])
        assert result.exit_code == 0
        assert "Observed Metrics" in result.output
