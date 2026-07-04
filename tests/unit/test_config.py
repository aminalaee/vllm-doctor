from pathlib import Path

import pytest
from sqlalchemy import make_url

from vllm_doctor.config import Config, default_vllm_doctor_db_url, load_config

FIXTURES = Path(__file__).parent.parent / "fixtures"


@pytest.fixture
def full_config(tmp_path):
    return load_config(FIXTURES / "full-config.toml")


@pytest.fixture(autouse=True)
def isolated_home(tmp_path, monkeypatch):
    monkeypatch.setenv("HOME", str(tmp_path / "home"))


class TestLoadConfig:
    def test_no_file_returns_defaults(self, tmp_path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        config = load_config()
        assert config == Config()

    def test_default_vllm_doctor_db_url(self):
        url = make_url(default_vllm_doctor_db_url())
        assert url.drivername == "sqlite"
        assert url.database is not None
        assert url.database.endswith("/.vllm-doctor/vllm_doctor.db")
        assert Path(url.database).parent.exists()

    def test_explicit_path_loads_file(self, tmp_path):
        toml = tmp_path / "cfg.toml"
        toml.write_text("[rules.kv_cache_pressure]\nhigh_cache_usage = 0.75\n")
        config = load_config(toml)
        assert config.rules.kv_cache_pressure.high_cache_usage == 0.75

    def test_database_url_loads_from_config(self, tmp_path):
        toml = tmp_path / "cfg.toml"
        toml.write_text('[database]\nurl = "sqlite:///:memory:"\n')
        config = load_config(toml)
        assert config.database.url == "sqlite:///:memory:"

    def test_partial_config_keeps_other_defaults(self, tmp_path):
        toml = tmp_path / "cfg.toml"
        toml.write_text("[rules.kv_cache_pressure]\nhigh_cache_usage = 0.75\n")
        config = load_config(toml)
        assert config.rules.queue_pressure.high_waiting == 5
        assert config.rules.error_rate.high_error_rate == 0.05

    def test_auto_discovers_local_toml(self, tmp_path, monkeypatch):
        monkeypatch.chdir(tmp_path)
        (tmp_path / "vllm-doctor.toml").write_text("[rules.queue_latency]\nhigh_queue_time_p95 = 2.5\n")
        config = load_config()
        assert config.rules.queue_latency.high_queue_time_p95 == 2.5

    def test_missing_explicit_path_raises(self, tmp_path):
        with pytest.raises(FileNotFoundError):
            load_config(tmp_path / "nonexistent.toml")

    def test_unknown_key_raises(self, tmp_path):
        toml = tmp_path / "cfg.toml"
        toml.write_text("[rules.kv_cache_pressure]\nhigh_cache_usge = 0.75\n")
        with pytest.raises(Exception):
            load_config(toml)

    def test_full_config_queue_pressure(self, full_config):
        assert full_config.rules.queue_pressure.high_waiting == 10
        assert full_config.rules.queue_pressure.high_running == 100

    def test_full_config_database(self, full_config):
        assert full_config.database.url == "sqlite:///:memory:"

    def test_full_config_queue_latency(self, full_config):
        assert full_config.rules.queue_latency.high_queue_time_p95 == 2.0

    def test_full_config_kv_cache_pressure(self, full_config):
        assert full_config.rules.kv_cache_pressure.high_cache_usage == 0.85

    def test_full_config_preemption_pressure(self, full_config):
        assert full_config.rules.preemption_pressure.high_cache_usage == 0.70

    def test_full_config_low_throughput(self, full_config):
        assert full_config.rules.low_throughput.low_prompt_tps == 5.0
        assert full_config.rules.low_throughput.low_gen_tps == 20.0
        assert full_config.rules.low_throughput.low_running == 1

    def test_full_config_error_rate(self, full_config):
        assert full_config.rules.error_rate.high_error_rate == 0.10
        assert full_config.rules.error_rate.high_abort_rate == 0.20

    def test_full_config_ttft_bottleneck(self, full_config):
        assert full_config.rules.ttft_bottleneck.high_ttft_p95 == 3.0
        assert full_config.rules.ttft_bottleneck.high_tpot_p95 == 0.3

    def test_full_config_tpot_bottleneck(self, full_config):
        assert full_config.rules.tpot_bottleneck.high_tpot_p95 == 0.3
        assert full_config.rules.tpot_bottleneck.low_gen_tokens_per_sec == 30.0

    def test_full_config_prefix_cache_efficiency(self, full_config):
        assert full_config.rules.prefix_cache_efficiency.min_hit_rate == 0.70

    def test_full_config_replica_imbalance(self, full_config):
        assert full_config.rules.replica_imbalance.imbalance_factor == 3.0
