from collections.abc import Generator
from pathlib import Path

import pytest
from typer.testing import CliRunner

from vllm_doctor.cli import app
from vllm_doctor.config import Config, DatabaseConfig


@pytest.fixture(autouse=True)
def config(monkeypatch: pytest.MonkeyPatch, db_url: str) -> Generator[Config, None, None]:
    cfg = Config(database=DatabaseConfig(url=db_url))

    def _load_config(_path: Path | None = None) -> Config:
        return cfg

    monkeypatch.setattr("vllm_doctor.cli.migrate.load_config", _load_config)
    return cfg


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


class TestMigrate:
    def test_migrate_is_idempotent(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["migrate"])
        assert result.exit_code == 0
        assert "Database migrated successfully." in result.output
