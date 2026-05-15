# Metrics

vLLM Doctor reads the following metrics from the vLLM `/metrics` endpoint or Prometheus.

## Supported metrics

| Metric                      | Description                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| `vllm:num_requests_running` | Number of requests currently being processed                                                                             |
| `vllm:num_requests_waiting` | Number of requests queued, waiting for capacity                                                                          |
| `vllm:gpu_cache_usage_perc` | Fraction of GPU KV cache currently in use (0.0–1.0); `n/a` on idle servers until at least one request has been processed |

## Notes

- Metric names use colons (e.g. `vllm:num_requests_running`), not underscores. vLLM Doctor preserves the original names — no normalization.
- All metrics are per model instance. If multiple models are running, values are summed across instances unless filtered by `model_name` label.
