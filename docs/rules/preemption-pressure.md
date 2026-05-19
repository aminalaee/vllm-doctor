# Preemption Pressure

Detects when vLLM has preempted sequences due to KV cache exhaustion.

## Background

Preemption happens when a running sequence must be evicted from GPU KV cache to free space for another. The evicted sequence is saved to CPU memory and re-computed later — wasting GPU cycles and adding latency spikes for the affected request.

Any preemptions indicate the server has run out of KV cache at least once. Frequent preemptions suggest the concurrent request mix consistently exceeds available cache capacity.

## Signals

| Signal            | Condition                                         |
| ----------------- | ------------------------------------------------- |
| Preemptions occur | `num_preemptions_total > 0`                       |
| Cache under load  | `kv_cache_usage_perc >= 0.80` (default threshold) |

## Confidence

| Signals matched                   | Confidence |
| --------------------------------- | ---------- |
| Preemptions only                  | Medium     |
| Preemptions + high KV cache usage | High       |

## Likely causes

- KV cache too small for the concurrent request mix
- Long-context requests exhausting cache before shorter ones complete
- `max_num_seqs` set too high relative to available GPU memory

## Recommendations

- Reduce `max_num_seqs` to limit concurrent sequences in GPU memory
- Reduce `max_num_batched_tokens` to lower per-step memory pressure
- Increase `gpu_memory_utilization` if GPU headroom exists
- Route long-context requests to a dedicated replica

## Metrics used

- `vllm:num_preemptions_total`
- `vllm:kv_cache_usage_perc`

## Configuration

| Setting          | Default |
| ---------------- | ------- |
| High cache usage | `0.80`  |
