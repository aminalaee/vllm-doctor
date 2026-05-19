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
```

## Partial config

Only the sections you care about need to be present. For example, to tighten only the KV cache threshold:

```toml
[rules.kv_cache_pressure]
high_cache_usage = 0.75
```

All other rules use their defaults.

## Usage

```bash
vllm-doctor --url http://localhost:9090 --config ./vllm-doctor.toml
```
