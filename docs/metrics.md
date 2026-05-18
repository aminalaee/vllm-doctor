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

## Notes

- Metric names use colons (e.g. `vllm:num_requests_running`), not underscores. vLLM Doctor preserves the original names — no normalization.
- All metrics are per model instance. If multiple models are running, values are summed across instances unless filtered by `model_name` label.
- Latency histograms (`time_to_first_token_seconds`, `request_time_per_output_token_seconds`) require Prometheus mode — direct scrape mode returns `None` for these fields.
- Request counts by reason (`error`, `abort`) are derived from `vllm:request_success_total` filtered by the `finished_reason` label.
