# Low Throughput

Detects when the server is processing requests below expected throughput with no queue pressure.

## Background

Low throughput with an empty queue means the server is underutilized — not overloaded. This is distinct from queue pressure, where low throughput is caused by saturation. Here, the server has capacity but is not using it, typically due to low incoming load, poor batching, or misconfigured concurrency limits.

## Signals

| Signal                   | Condition                                     |
| ------------------------ | --------------------------------------------- |
| Prefill throughput low   | `prompt_tokens_per_second < 10` (default)     |
| Decode throughput low    | `generation_tokens_per_second < 50` (default) |
| Very few active requests | `num_requests_running < 2` (default)          |

This rule is suppressed when `num_requests_waiting > 0` — a queue means the low throughput is a capacity problem, not underutilization.

## Confidence

| Signals matched                                  | Confidence |
| ------------------------------------------------ | ---------- |
| Both prefill and decode low, or very few running | Medium     |
| Only one metric low                              | Low        |

## Likely causes

- Low incoming request rate — server is idle
- Poor batching due to few concurrent requests
- `max_num_seqs` or `max_num_batched_tokens` configured too conservatively for current load

## Recommendations

- Increase concurrent requests to improve batching efficiency
- Review `max_num_seqs` and `max_num_batched_tokens` settings
- Compare against a benchmark baseline to confirm underperformance
- Consider consolidating replicas if load is consistently low

## Metrics used

- `vllm:prompt_tokens_per_second`
- `vllm:generation_tokens_per_second`
- `vllm:num_requests_running`

## Configuration

```toml
[rules.low_throughput]
low_prompt_tps = 20.0    # default: 10.0
low_gen_tps = 100.0      # default: 50.0
low_running = 3          # default: 2
```
