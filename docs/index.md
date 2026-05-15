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

=== "Prometheus"

    ```shell
    vllm-doctor --url http://localhost:9090
    ```

=== "JSON output"

    ```shell
    vllm-doctor --url http://localhost:8000/metrics --format json
    ```

## Example output

```shell
─────────── vLLM Doctor  ·  Health: CRITICAL  ·  Window: now ───────────

✖ KV cache pressure  [high confidence]
  Cache saturation blocking new request admission

  GPU KV cache usage: 94% (threshold: 90%)
  Waiting requests: 7 (blocked by full cache)

  → Reduce max_num_seqs to limit concurrent sequences
  → Reduce max_num_batched_tokens to cap memory per step
  → Increase gpu_memory_utilization if GPU memory headroom exists
  → Route long-context requests to a dedicated replica

⚠ Queue pressure  [low confidence]
  Waiting requests: 7 (threshold: 5)

  → Add replicas or increase concurrency limits
  → Inspect autoscaling thresholds

────────────────────────── Observed Metrics ─────────────────────────────

  Requests Running   12
  Requests Waiting    7
  GPU Cache Usage   94%
```
