import pytest
from typer.testing import CliRunner

from vllm_doctor.cli import app


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


class TestHistory:
    def test_history_help_shows_commands(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["history", "--help"])
        assert result.exit_code == 0
        assert "Local diagnosis history commands" in result.output

    def test_history_in_root_help(self, runner: CliRunner) -> None:
        result = runner.invoke(app, ["--help"])
        assert result.exit_code == 0
        assert "history" in result.output
