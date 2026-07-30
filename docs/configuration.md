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

After changing `url` or installing vLLM Doctor for the first time, run
[`vllm-doctor migrate`](../commands/migrate.md) once to initialize or update
the history database. The command is safe to run repeatedly.

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

| Key              | Default | Description                                                     |
| ---------------- | ------- | --------------------------------------------------------------- |
| `id`             | —       | Stable name chosen for this target. Required for cloud upload.  |
| `engine`         | `vllm`  | Inference engine. Currently, the only accepted value is `vllm`. |
| `engine_version` | —       | Engine version string (e.g. `"0.8.0"`).                         |
| `environment`    | —       | Environment label (e.g. `production`, `staging`).               |

Use the same ID whenever you diagnose this target. On its first upload, vLLM
Doctor Cloud creates the target in the account authorized by the observation
token. Local diagnosis works without an ID.

## Agent observability

During watch mode, vLLM Doctor can expose its own health, readiness, and Prometheus metrics over HTTP. No listener is enabled by default.

```toml
[agent]
listen = "127.0.0.1:9091"
id = "019c..."
```

| Key      | Default        | Description                                                   |
| -------- | -------------- | ------------------------------------------------------------- |
| `listen` | —              | Socket address serving `/healthz`, `/readyz`, and `/metrics`. |
| `id`     | Generated once | Stable identity for this Doctor installation.                 |

When `id` is omitted, vLLM Doctor generates and reuses one automatically.

The `--listen` command-line option overrides this setting. The configured address is only used by `diagnose --watch`; one-shot diagnosis does not start a server. The endpoints do not provide authentication or TLS, so prefer a loopback or otherwise protected address. Binding to `0.0.0.0` exposes operational data to the network.

## Cloud upload

Cloud upload is disabled by default. Opt in with `--upload` or set
`enabled = true` for unattended watch mode.

```toml
[upload]
timeout = 15
enabled = false
```

| Key       | Default | Description                                    |
| --------- | ------- | ---------------------------------------------- |
| `timeout` | `15`    | Upload request timeout in seconds.             |
| `enabled` | `false` | Send every successful diagnosis automatically. |

Provide the cloud token through `VLLM_DOCTOR_TOKEN`. Tokens are not accepted as
command-line or configuration values.

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
