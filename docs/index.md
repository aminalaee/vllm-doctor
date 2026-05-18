<img src="assets/wordmark.svg" alt="vLLM Doctor" class="vd-wordmark">

Diagnose vLLM serving issues from `/metrics`.

vLLM Doctor reads production metrics and turns them into operational findings: what looks wrong, how confident the diagnosis is, and which vLLM knobs are worth checking first.

```shell
vllm-doctor --url http://localhost:8000/metrics
```

!!! note "Built for incident context"
    vLLM Doctor is not a dashboard replacement. It is a fast diagnostic snapshot for a single server or Prometheus target.

## Why not just a dashboard?

Dashboards show metrics. vLLM Doctor explains inference-system behavior.

|                          | Dashboards | vLLM Doctor |
| ------------------------ | ---------- | ----------- |
| Shows raw metrics        | ✓          | ✓           |
| Explains what's wrong    | ✗          | ✓           |
| Recommends vLLM configs  | ✗          | ✓           |
| Requires setup           | ✓          | ✗           |
| Works on a single server | ✗          | ✓           |

## Installation

=== "pip"

    ```shell
    pip install vllm-doctor
    ```

=== "uv"

    ```shell
    uv tool install vllm-doctor
    ```

## Quickstart

=== "Direct scrape"

    ```shell
    vllm-doctor --url http://localhost:8000/metrics
    ```

    !!! note
        Direct scrape mode reads instant gauge values. Latency percentile rules (TTFT, TPOT) are not available — use Prometheus mode for full diagnosis.

=== "Prometheus"

    ```shell
    vllm-doctor --url http://localhost:9090
    ```

=== "JSON output"

    ```shell
    vllm-doctor --url http://localhost:8000/metrics --format json
    ```

=== "Verbose"

    ```shell
    vllm-doctor --url http://localhost:8000/metrics --verbose
    ```

## Example output

```shell
─────────── vLLM Doctor  ·  Health: CRITICAL  ·  Window: 5m ────────────

╭─ ✖ KV cache pressure  [high confidence] ─────────────────────────────╮
│   GPU KV cache usage: 94%  ·  Waiting requests: 7                    │
│                                                                      │
│   → Reduce max_num_seqs to limit concurrent sequences                │
│   → Increase gpu_memory_utilization if GPU memory headroom exists    │
╰──────────────────────────────────────────────────────────────────────╯
╭─ ⚠ Queue pressure  [low confidence] ─────────────────────────────────╮
│   Waiting requests: 7                                                │
│                                                                      │
│   → Add replicas or increase concurrency limits                      │
│   → Inspect autoscaling thresholds                                   │
╰──────────────────────────────────────────────────────────────────────╯

─────────────────────────── Observed Metrics ───────────────────────────

  Requests Running                             12
  Requests Waiting                              7
  GPU Cache Usage        ███████████████████░ 94%
  Generation Tokens/s                        42.0
  TTFT p95 (s)                              3.200
  TPOT p95 (s)                              0.050
```
