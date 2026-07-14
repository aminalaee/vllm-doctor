<img src="https://raw.githubusercontent.com/aminalaee/vllm-doctor/main/docs/assets/wordmark.svg" alt="vLLM Doctor" width="360">

Diagnose vLLM server bottlenecks from live metrics.

![vllm-doctor demo](https://raw.githubusercontent.com/aminalaee/vllm-doctor/main/docs/demo.png)

vLLM Doctor reads vLLM server metrics and turns them into a diagnosis: the most likely bottleneck up front, followed by the detailed findings — what looks unhealthy, why it may be happening, and which vLLM settings are worth checking first.

```shell
vllm-doctor diagnose http://localhost:8000/metrics
```

> vLLM Doctor is not a dashboard replacement or benchmark runner. It is a fast server-side diagnostic snapshot for a single vLLM server or Prometheus target.

## Features

- **Root-cause summary** — a "Likely bottleneck" triage of the most probable cause, shown above the detailed findings
- **Built-in diagnosis rules** — queue pressure, TTFT/TPOT bottlenecks, KV cache pressure, low throughput, error rate, replica imbalance, prefix cache efficiency, preemption pressure, queue latency
- **Local history** — persist runs with `--save`, review with `history list` and `history show`
- **Watch change-log** — `--save --watch` only persists when state changes (health or firing rules)
- **Dual input** — Prometheus or direct `/metrics` scrape
- **Structured output** — rich text tables or JSON for automation
- **Configurable thresholds** — per-rule tuning via TOML

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

Quick install (Linux & macOS):

```shell
curl -fsSL https://raw.githubusercontent.com/aminalaee/vllm-doctor/main/scripts/install.sh | sh
```

With Cargo:

```shell
cargo install vllm-doctor
```

With Docker:

```shell
docker run --rm ghcr.io/aminalaee/vllm-doctor diagnose <url>
```

## Quickstart

Direct scrape:

```shell
vllm-doctor diagnose http://localhost:8000/metrics
```

Prometheus:

```shell
vllm-doctor diagnose http://localhost:9090
```

To try the diagnosis without installing vLLM, run the
[mock metrics example](examples/mock/). To test against a real local model, use
the [live vLLM example](examples/live-vllm/).

## Run with Docker

A prebuilt image is published to GitHub Container Registry:

```shell
docker run --rm ghcr.io/aminalaee/vllm-doctor diagnose <url>
```

`<url>` is your vLLM `/metrics` or Prometheus endpoint — the same argument the CLI takes — reachable from inside the container.

## Example output

Default output is compact — the bottleneck up front, one line per finding, and a count of passing checks. Use `--verbose` (`-v`) for the full evidence, recommendations, metrics tables, and notices.

```shell
$ vllm-doctor diagnose http://localhost:8000/metrics
────────────────────────────────────────────────────────────────────────────────
                vLLM Doctor  ·  Health: CRITICAL  ·  Since: now                 
────────────────────────────────────────────────────────────────────────────────

Likely bottleneck: KV cache saturation (high confidence)
  Cache is full (94%), so requests queue and TTFT climbs. The queue and TTFT
  findings are likely downstream — start with the KV cache.

  ✖ KV cache pressure     GPU KV cache usage: 94% (>90%), 7 requests waiting
  ⚠ High TTFT             TTFT p95: 3.2s, but TPOT p95 is 0.05s — prefill bound
  ⚠ Replica imbalance     vllm-1: 10 running / 94% cache vs vllm-0: 2 / 41%
  ⚠ Queue pressure        7 requests waiting (threshold: 5)

6 checks passed · run -v for evidence, fixes, and per-replica metrics
```

With `--verbose` (`-v`) the same run expands each finding into full detail — evidence, recommended actions, observed metrics, and any notices — for deeper inspection.

```shell
$ vllm-doctor diagnose -v http://localhost:8000/metrics
────────────────────────────────────────────────────────────────────────────────
                vLLM Doctor  ·  Health: CRITICAL  ·  Since: now                 
────────────────────────────────────────────────────────────────────────────────

Likely bottleneck: KV cache saturation (high confidence)
  Cache is full (94%), so requests queue and TTFT climbs. The queue and TTFT
  findings are likely downstream — start with the KV cache.

Findings

╭──────────────────────────────────────────────────────────────────────────────╮
│              ✖ KV cache pressure  [high]   GPU cache 94%, 7 waiting          │
│                                                                              │
│  GPU KV cache usage: 94% (threshold: 90%)  ·  Waiting requests: 7             │
│                                                                              │
│  → Reduce max_num_seqs to limit concurrent sequences                         │
│  → Reduce max_num_batched_tokens to cap memory per step                      │
│  → Increase gpu_memory_utilization if GPU memory headroom exists             │
│  → Route long-context requests to a dedicated replica                        │
╰──────────────────────────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────────────────────────╮
│           ⚠ High time to first token (TTFT)  [high]   TTFT 3.2s, TPOT 0.05s  │
│                                                                              │
│  TTFT p95: 3.200s  ·  TPOT p95: 0.050s  ·  Waiting requests: 7               │
│                                                                              │
│  → Enable or tune chunked prefill (--enable-chunked-prefill)                 │
│  → Reduce max prompt length or filter long requests                          │
│  → Inspect queue depth — consider adding replicas                            │
│  → Separate long-context traffic to dedicated instances                      │
╰──────────────────────────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────────────────────────╮
│              ⚠ Replica imbalance  [high]   vllm-1 hot, vllm-0 idle            │
│                                                                              │
│  meta-llama/Llama-3.1-8B: running vllm-1=10 vs vllm-0=2; cache 94% vs 41%;   │
│  waiting vllm-1=7 vs vllm-0=0                                                │
│                                                                              │
│  → Check the load balancer / service routing and session affinity settings   │
│  → Verify readiness probes — an unready replica receives no traffic          │
│  → Compare per-replica latency and restart any unhealthy replica             │
│  → Confirm newly added replicas are registered with the load balancer        │
╰──────────────────────────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────────────────────────╮
│              ⚠ Queue pressure  [low]   7 waiting (threshold: 5)              │
│                                                                              │
│  Waiting requests: 7 (threshold: 5)                                          │
│                                                                              │
│  → Add replicas or increase concurrency limits                               │
│  → Inspect autoscaling thresholds                                            │
│  → Separate long-context traffic to a dedicated replica                      │
│  → Reduce incoming request rate                                              │
╰──────────────────────────────────────────────────────────────────────────────╯

Passed

 ✓ Queue Latency       ✓ Preemption Pressure   ✓ Low Throughput        
 ✓ Error Rate            ✓ High TPOT             ✓ Prefix Cache Efficiency 

⚠ TTFT, TPOT and Queue Latency rules require Prometheus — connect to Prometheus for full analysis.

Observed Metrics:

 Metric                                    Value 
 Requests Running                             12 
 Requests Waiting                              7 
 GPU Cache Usage        ███████████████████░ 94% 
 Prefill Tokens/s                          390.0 
 Decode Tokens/s                           252.0 
 Requests Success                            114 
 Requests Error                                0 
 Requests Aborted                              0 
 TTFT p95 (s)                              3.200 
 TPOT p95 (s)                              0.050 
 Queue Time p95 (s)                        0.800 
 Preemptions Total                             0 
 Prefix Cache Hit Rate                       50% 

Observed Metrics per pod:

                    vllm-1  vllm-0 
 Requests Running       10       2 
 Requests Waiting        7       0 
 GPU Cache Usage       94%     41% 
 Prefill Tokens/s     80.0   310.0 
 Decode Tokens/s      42.0   210.0 
 Requests Success       30      84 
 Requests Error          0       0 
 Requests Aborted        0       0 
 Preemptions Total       0       0 
```

## Documentation

Read the full documentation: https://aminalaee.github.io/vllm-doctor
