# High TPOT (Time Per Output Token)

Detects when the p95 time per output token exceeds the configured threshold.

## Background

TPOT measures how long each output token takes to generate during the decode phase. High TPOT indicates that the GPU is struggling to keep up with the decode workload — typically due to memory bandwidth saturation or too many concurrent sequences.

Unlike TTFT, which reflects prefill or queue pressure, high TPOT with normal TTFT isolates the bottleneck to the decode phase.

!!! note "Prometheus mode only"
    TPOT percentiles require `histogram_quantile()` over a time window. This rule does not fire in direct scrape mode.

## Signals

| Signal                    | Condition                                                       |
| ------------------------- | --------------------------------------------------------------- |
| High TPOT                 | `tpot_p95_seconds >= 0.2` (default)                             |
| Low generation throughput | `generation_tokens_per_second < 50` — decode pressure confirmed |
| TTFT normal               | `ttft_p95_seconds < 2.0` — bottleneck is in decode, not prefill |

## Confidence

| Signals matched                              | Confidence |
| -------------------------------------------- | ---------- |
| High TPOT only                               | Low        |
| High TPOT + low generation throughput        | Medium     |
| High TPOT + low gen throughput + normal TTFT | High       |

## Likely causes

- GPU memory bandwidth saturated during decode
- Too many concurrent sequences reducing per-request throughput
- Large model size relative to available GPU memory
- Insufficient tensor parallelism for current load

## Recommendations

- Reduce max concurrent requests (`--max-num-seqs`)
- Increase tensor parallelism to distribute decode across GPUs
- Enable speculative decoding to amortize decode cost
- Profile GPU memory bandwidth utilization

## Metrics used

- `vllm:request_time_per_output_token_seconds` (histogram)
- `vllm:generation_tokens_per_second`
- `vllm:time_to_first_token_seconds` (histogram)

## Configuration

| Setting                         | Default    |
| ------------------------------- | ---------- |
| TPOT p95 threshold              | `0.2s`     |
| Generation throughput threshold | `50 tok/s` |
