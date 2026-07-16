# Configuration

vLLM Doctor can be configured via a TOML file. All settings are optional — omitted values use the defaults shown below.

## Config file location

vLLM Doctor looks for a config file in this order:

1. Path passed via `--config` flag
2. `./vllm-doctor.toml` (current directory)
3. `~/.config/vllm-doctor/config.toml`

If none is found, all defaults apply.

## Example config

```toml
[rules.queue_pressure]
high_waiting = 5      # fire when waiting requests exceed this
high_running = 50     # corroborate when running requests exceed this

[rules.queue_latency]
high_queue_time_p95 = 1.0   # seconds

[rules.kv_cache_pressure]
high_cache_usage = 0.90     # fraction (0.0–1.0)

[rules.preemption_pressure]
high_cache_usage = 0.80     # fraction (0.0–1.0)

[rules.low_throughput]
low_prompt_tps = 10.0       # prompt tokens/s
low_gen_tps = 50.0          # generation tokens/s
low_running = 2             # requests running

[rules.error_rate]
high_error_rate = 0.05      # fraction of total requests
high_abort_rate = 0.10      # fraction of total requests

[rules.ttft_bottleneck]
high_ttft_p95 = 2.0         # seconds
high_tpot_p95 = 0.2         # seconds (used to confirm decode is not the bottleneck)

[rules.tpot_bottleneck]
high_tpot_p95 = 0.2                # seconds
low_gen_tokens_per_sec = 50.0      # corroborating signal

[rules.prefix_cache_efficiency]
min_hit_rate = 0.50         # fraction (0.0–1.0)

[rules.replica_imbalance]
imbalance_factor = 2.0      # busiest / least-busy running ratio
cache_gap = 0.30            # kv cache usage max − min (fraction)
min_total_running = 5.0     # minimum total running load before the running signal fires
```

## Database

History persistence is configured under a `[database]` section. The single setting is `url`, a SQLite database URL.

```toml
[database]
url = "sqlite:///~/.vllm-doctor/vllm_doctor.db"
```

| Key   | Default                                   | Description                                                          |
| ----- | ----------------------------------------- | -------------------------------------------------------------------- |
| `url` | `sqlite:///~/.vllm-doctor/vllm_doctor.db` | SQLite URL — local file path. The directory is created on first run. |

After changing `url` (or after installing vllm-doctor for the first time), run [`vllm-doctor migrate`](../commands/migrate.md) once to create or update the schema. The command is idempotent.

See the [history guide](../commands/history.md) for the full save / watch change-log / list / show loop.

## Target

The `[target]` section identifies the inference engine and deployment being diagnosed. All fields are optional except `engine`, which defaults to `vllm`.

```toml
[target]
id = "llama-serving-prod"
engine = "vllm"
engine_version = "0.8.0"
environment = "production"
```

| Key              | Default | Description                                                                                          |
| ---------------- | ------- | ---------------------------------------------------------------------------------------------------- |
| `id`             | —       | Stable, operator-provided target identifier. Optional for local CLI; must be stable when configured. |
| `engine`         | `vllm`  | Inference engine. Currently, the only accepted value is `vllm`.                                      |
| `engine_version` | —       | Engine version string (e.g. `"0.8.0"`).                                                              |
| `environment`    | —       | Environment label (e.g. `production`, `staging`).                                                    |

An empty or whitespace-only `id` is rejected at load time. When `id` is absent the CLI does not generate one — a later SaaS enrollment change will require or generate a stable ID before upload.

## Partial config

Only the sections you care about need to be present. For example, to tighten only the KV cache threshold:

```toml
[rules.kv_cache_pressure]
high_cache_usage = 0.75
```

All other rules use their defaults.

## Usage

```bash
vllm-doctor diagnose http://localhost:9090 --config ./vllm-doctor.toml
```
