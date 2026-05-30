# High TTFT (Time to First Token)

Detects when the p95 time to first token exceeds the configured threshold.

## Background

TTFT measures how long a client waits before receiving the first token. High TTFT indicates that requests are spending too long in prefill or in the waiting queue before prefill even begins.

Unlike TPOT, which reflects decode throughput, high TTFT with stable TPOT strongly suggests the bottleneck is in prefill or queue pressure — not decode.

In benchmark reports, TTFT is usually client-observed. vLLM Doctor reads the server-side vLLM TTFT histogram, then uses queue depth and TPOT to explain whether the delay looks like admission pressure, prefill pressure, or a broader serving bottleneck.

!!! note "Prometheus mode only"
    TTFT percentiles require `histogram_quantile()` over a time window. This rule does not fire in direct scrape mode.

## Signals

| Signal          | Condition                                               |
| --------------- | ------------------------------------------------------- |
| High TTFT       | `ttft_p95_seconds >= 2.0` (default)                     |
| TPOT stable     | `tpot_p95_seconds < 0.2` — decode is not the bottleneck |
| Requests queued | `num_requests_waiting > 0` — prefill pressure confirmed |

## Confidence

| Signals matched                       | Confidence |
| ------------------------------------- | ---------- |
| High TTFT only                        | Low        |
| High TTFT + stable TPOT               | Medium     |
| High TTFT + stable TPOT + queue depth | High       |

## Likely causes

- Long input prompts increasing prefill compute time
- Queue pressure delaying prefill start
- Chunked prefill not enabled or misconfigured
- Insufficient capacity for current prompt load

## Recommendations

- Enable or tune chunked prefill (`--enable-chunked-prefill`)
- Reduce max prompt length or filter long requests upstream
- Inspect queue depth — consider adding replicas
- Separate long-context traffic to dedicated instances

## Metrics used

- `vllm:time_to_first_token_seconds` (histogram)
- `vllm:request_time_per_output_token_seconds` (histogram)
- `vllm:num_requests_waiting`

## Configuration

| Setting            | Default |
| ------------------ | ------- |
| TTFT p95 threshold | `2.0s`  |
| TPOT p95 threshold | `0.2s`  |
