import uuid
from collections.abc import Iterator

import pytest
from sqlalchemy import URL

from vllm_doctor.clients.models import MetricSample
from vllm_doctor.config import Config
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


@pytest.fixture
def result() -> DiagnosisResult:
    finding = Finding(
        severity=Severity.warning,
        confidence=Confidence.low,
        title="Queue pressure",
        summary="Requests are queuing faster than the server can process them.",
        evidence=["Waiting requests: 7"],
    )
    return DiagnosisResult(
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


@pytest.fixture
def result_without_model(result: DiagnosisResult) -> DiagnosisResult:
    return result.model_copy(update={"context": result.context.model_copy(update={"model_name": None})})


@pytest.fixture
def store(db_url: str) -> Iterator[HistoryStore]:
    run_migrations(db_url)
    s = HistoryStore(db_url)
    yield s
    s.close()


class TestHistoryStore:
    def test_save_returns_uuid7(self, store: HistoryStore, result: DiagnosisResult) -> None:
        run_id = store.save(result)
        assert uuid.UUID(run_id).version == 7

    def test_save_get_round_trip(self, store: HistoryStore, result: DiagnosisResult) -> None:
        run_id = store.save(result)
        loaded = store.get(run_id)

        assert loaded is not None
        assert loaded.context.model_name == "meta-llama/Llama-3.1-8B"
        assert loaded.context.client_mode == ClientMode.prometheus
        assert loaded.health == Severity.warning.value
        assert loaded.metrics.num_requests_running == 5
        assert loaded.metrics.num_requests_waiting == 7
        assert [c.id for c in loaded.checks] == ["queue_pressure", "kv_cache_pressure"]
        assert loaded.checks[0].finding is not None
        assert loaded.checks[0].finding.title == "Queue pressure"

    def test_get_missing_returns_none(self, store: HistoryStore) -> None:
        assert store.get("does-not-exist") is None

    def test_list_returns_metadata(self, store: HistoryStore, result: DiagnosisResult) -> None:
        run_id = store.save(result)
        runs = store.list()

        assert len(runs) == 1
        meta = runs[0]
        assert meta.run_id == run_id
        assert meta.model_name == "meta-llama/Llama-3.1-8B"
        assert meta.client_mode == ClientMode.prometheus
        assert meta.health.value == "warning"
        assert meta.fired_count == 1
        assert meta.saved_at

    def test_list_newest_first(self, store: HistoryStore, result: DiagnosisResult) -> None:
        first = store.save(result)
        second = store.save(result)
        assert [m.run_id for m in store.list()] == [second, first]

    def test_null_model_name(self, store: HistoryStore, result_without_model: DiagnosisResult) -> None:
        run_id = store.save(result_without_model)
        assert store.list()[0].model_name is None
        assert store.get(run_id).context.model_name is None

    def test_store_persists_across_reopen(self, db_url: str, result: DiagnosisResult) -> None:
        run_migrations(db_url)
        with HistoryStore(db_url) as store:
            run_id = store.save(result)

        with HistoryStore(db_url) as reopened:
            assert reopened.get(run_id) is not None

    def test_file_backed_url_with_query_character(self, tmp_path, result: DiagnosisResult) -> None:
        db = tmp_path / "a?b.db"
        url = URL.create("sqlite", database=str(db))
        run_migrations(url)
        with HistoryStore(url) as store:
            run_id = store.save(result)

        assert db.exists()
        with HistoryStore(url) as reopened:
            assert reopened.get(run_id) is not None

    def test_default_config_url_creates_local_database(
        self, tmp_path, monkeypatch: pytest.MonkeyPatch, result: DiagnosisResult
    ) -> None:
        home = tmp_path / "home"
        monkeypatch.setenv("HOME", str(home))
        db = home / ".vllm-doctor" / "vllm_doctor.db"

        run_migrations(Config().database.url)
        with HistoryStore(Config().database.url) as store:
            run_id = store.save(result)

        assert db.exists()
        with HistoryStore(Config().database.url) as reopened:
            assert reopened.get(run_id) is not None
