<img src="https://raw.githubusercontent.com/aminalaee/vllm-doctor/main/docs/assets/wordmark.svg" alt="vLLM Doctor" width="360">

<p>
<a href="https://pypi.org/project/vllm-doctor/">
    <img src="https://badge.fury.io/py/vllm-doctor.svg" alt="Package version">
</a>
<a href="https://pypi.org/project/vllm-doctor/">
    <img src="https://img.shields.io/pypi/pyversions/vllm-doctor.svg?color=%2334D058" alt="Supported Python versions">
</a>
</p>

Diagnose vLLM server bottlenecks from live metrics.

![vllm-doctor demo](https://raw.githubusercontent.com/aminalaee/vllm-doctor/main/docs/demo.png)

vLLM Doctor reads vLLM server metrics and turns them into diagnostic findings: what looks unhealthy, why it may be happening, and which vLLM settings are worth checking first.

```shell
vllm-doctor --url http://localhost:8000/metrics
```

> vLLM Doctor is not a dashboard replacement or benchmark runner. It is a fast server-side diagnostic snapshot for a single vLLM server or Prometheus target.

## Why not just a dashboard?

Dashboards show metrics. vLLM Doctor explains server-side inference behavior.

|                          | Dashboards | vLLM Doctor |
| ------------------------ | ---------- | ----------- |
| Shows raw metrics        | ✓          | ✓           |
| Explains what's wrong    | ✗          | ✓           |
| Recommends vLLM configs  | ✗          | ✓           |
| Requires setup           | ✓          | ✗           |
| Works on a single server | ✗          | ✓           |

## How does this relate to GuideLLM?

GuideLLM is a good fit for generating workloads and measuring endpoint behavior. vLLM Doctor is a good fit for explaining server-side symptoms from vLLM metrics.

Used together, GuideLLM can create or replay load while vLLM Doctor helps explain bottlenecks such as queue pressure, KV cache pressure, high TTFT, or high TPOT.

## Installation

With pip:

```shell
pip install vllm-doctor
```

With uv:

```shell
uv tool install vllm-doctor
```

## Quickstart

Direct scrape:

```shell
vllm-doctor --url http://localhost:8000/metrics
```

Prometheus:

```shell
vllm-doctor --url http://localhost:9090
```

Options:

```
Usage: vllm-doctor [OPTIONS]

Options:
  -u, --url      TEXT         URL to diagnose (vLLM /metrics or Prometheus).  [required]
  -s, --since    TEXT         Time window (e.g. '1h', '30m', 'now').  [default: now]
  -w, --watch                 Refresh continuously every 5s (pipe through `watch -n N` for a different interval).
  -o, --output   [text|json]  Output format.  [default: text]
  -v, --verbose               Show additional diagnostic detail.
  -c, --config   PATH         Path to config file (default: vllm-doctor.toml).
      --help                  Show this message and exit.
```

## Example verbose output

```shell
─────────── vLLM Doctor  ·  Health: CRITICAL  ·  Since: 5m ────────────

╭─ ⚠ Queue pressure  [low confidence] ─────────────────────────────────╮
│   Waiting requests: 7                                                │
│                                                                      │
│   → Add replicas or increase concurrency limits                      │
│   → Inspect autoscaling thresholds                                   │
╰──────────────────────────────────────────────────────────────────────╯
╭─ ✖ KV cache pressure  [high confidence] ─────────────────────────────╮
│   GPU KV cache usage: 94%  ·  Waiting requests: 7                    │
│                                                                      │
│   → Reduce max_num_seqs to limit concurrent sequences                │
│   → Increase gpu_memory_utilization if GPU memory headroom exists    │
╰──────────────────────────────────────────────────────────────────────╯

  Queue Pressure       ⚠ warning     [low]
  KV Cache Pressure    ✖ critical    [high]
  Low Throughput       ✓ ok
  Error Rate           ✓ ok
  High TTFT            ✓ ok

─────────────────────────── Observed Metrics ───────────────────────────

  Summary
  Requests Running                             12
  Requests Waiting                              7
  GPU Cache Usage        ███████████████████░ 94%
  Prefill Tokens/s                          390.0
  Decode Tokens/s                           252.0
  TTFT p95 (s)                              3.200
  TPOT p95 (s)                              0.050

─────────────────────── Observed Metrics per pod ───────────────────────
                          vllm-1    vllm-0
  Requests Running            10         2
  Requests Waiting             7         0
  GPU Cache Usage            94%       41%
  Prefill Tokens/s          80.0       310
  Decode Tokens/s           42.0       210
```

## Documentation

Read the full documentation: https://aminalaee.github.io/vllm-doctor
