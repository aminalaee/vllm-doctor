# Metrics

vLLM Doctor reads the following metrics from the vLLM `/metrics` endpoint or Prometheus.

## Supported metrics

| Metric                                       | Description                                                                                                              | Mode       |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------- |
| `vllm:num_requests_running`                  | Number of requests currently being processed                                                                             | Both       |
| `vllm:num_requests_waiting`                  | Number of requests queued, waiting for capacity                                                                          | Both       |
| `vllm:kv_cache_usage_perc`                   | Fraction of GPU KV cache currently in use (0.0–1.0); `n/a` on idle servers until at least one request has been processed | Both       |
| `vllm:prompt_tokens_per_second`              | Prompt tokens processed per second (prefill throughput)                                                                  | Both       |
| `vllm:generation_tokens_per_second`          | Output tokens generated per second (decode throughput)                                                                   | Both       |
| `vllm:request_success_total`                 | Cumulative finished requests, broken down by `finished_reason` label (`stop`, `error`, `abort`)                          | Both       |
| `vllm:time_to_first_token_seconds`           | Histogram of time from request arrival to first output token                                                             | Prometheus |
| `vllm:request_time_per_output_token_seconds` | Histogram of time per output token during decode                                                                         | Prometheus |
| `vllm:prefix_cache_hits_total`               | Cumulative prefix cache hits in tokens                                                                                   | Both       |
| `vllm:prefix_cache_queries_total`            | Cumulative prefix cache queries in tokens                                                                                | Both       |
| `vllm:request_queue_time_seconds`            | Histogram of time spent in the WAITING phase before prefill begins                                                       | Prometheus |
| `vllm:num_preemptions_total`                 | Cumulative number of sequence preemptions due to KV cache exhaustion                                                     | Both       |

## Client vs server terminology

GuideLLM and other benchmark tools often describe endpoint behavior with client-observed terms such as ITL, E2E latency, request latency, and workload throughput.

vLLM Doctor uses server-side vLLM metrics. The closest mappings are:

| Client or benchmark term | vLLM Doctor term       | Server metric                                   |
| ------------------------ | ---------------------- | ----------------------------------------------- |
| TTFT                     | TTFT p95               | `vllm:time_to_first_token_seconds`              |
| ITL                      | TPOT p95               | `vllm:request_time_per_output_token_seconds`    |
| E2E latency              | Not diagnosed directly | Explained through queue, TTFT, TPOT, and errors |
| Prompt throughput        | Prefill throughput     | `vllm:prompt_tokens_per_second`                 |
| Output throughput        | Decode throughput      | `vllm:generation_tokens_per_second`             |
| Admission delay          | Queue latency          | `vllm:request_queue_time_seconds`               |

## Notes

- Metric names use colons (e.g. `vllm:num_requests_running`), not underscores. vLLM Doctor preserves the original names — no normalization.
- Metrics preserve labels internally. Reported scalar values use each metric's diagnostic aggregation: counts and throughput are summed, while utilization and percentile-style values use the highest observed value. Use the model filter to scope collection to one `model_name`.
- Latency histograms (`time_to_first_token_seconds`, `request_time_per_output_token_seconds`) require Prometheus mode — direct scrape mode returns `None` for these fields.
- Request counts by reason (`error`, `abort`) are derived from `vllm:request_success_total` filtered by the `finished_reason` label.
- Prefix cache hit rate is computed as `prefix_cache_hits_total / prefix_cache_queries_total`. It is `n/a` when no queries have been made.
