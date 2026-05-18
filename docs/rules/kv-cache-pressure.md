# KV Cache Pressure

Detects when GPU KV cache is near exhaustion.

## Background

The KV cache stores intermediate attention computations (keys and values) for each active sequence on GPU. When the cache fills up, vLLM cannot admit new sequences — requests stall in the waiting queue even if GPU compute is otherwise available.

Classic failure mode: a few long-context requests fill the cache, blocking all other short requests behind them.

## Signals

| Signal               | Condition                                      |
| -------------------- | ---------------------------------------------- |
| Cache near full      | `kv_cache_usage_perc >= 0.90` (default)        |
| Cache blocking queue | `num_requests_waiting > 0` while cache is full |

## Confidence

| Signals matched               | Confidence |
| ----------------------------- | ---------- |
| Cache high only               | Medium     |
| Cache high + requests waiting | High       |

## Likely causes

- Long-context requests holding large KV cache allocations
- `max_num_seqs` or `max_num_batched_tokens` set too high for available GPU memory
- Sudden spike in concurrent requests

## Recommendations

- Reduce `max_num_seqs` to limit concurrent sequences
- Reduce `max_num_batched_tokens` to cap memory per step
- Increase `gpu_memory_utilization` if GPU memory headroom exists
- Route long-context requests to a dedicated replica

## Metrics used

- `vllm:kv_cache_usage_perc`
- `vllm:num_requests_waiting`

## Configuration

This threshold is currently fixed in the CLI:

| Setting                  | Default |
| ------------------------ | ------- |
| KV cache usage threshold | `0.90`  |

CLI flags for threshold overrides are planned.
