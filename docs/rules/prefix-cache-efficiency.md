# Prefix Cache Efficiency

Detects when the prefix cache hit rate is low despite requests being served.

## Background

vLLM's prefix cache reuses KV computations for repeated prompt prefixes — system prompts, few-shot examples, or shared context. When requests share a common prefix, vLLM skips recomputing it on every request, reducing TTFT and GPU load.

A low hit rate means those savings are not being realized. This is often a configuration oversight: prefix caching is disabled by default in some vLLM versions.

## Signals

| Signal       | Condition                                          |
| ------------ | -------------------------------------------------- |
| Low hit rate | `prefix_cache_hit_rate < 0.50` (default threshold) |

## Confidence

| Hit rate  | Confidence |
| --------- | ---------- |
| < 20%     | High       |
| 20% – 50% | Medium     |

## Likely causes

- Prefix caching not enabled (`--enable-prefix-caching` not set)
- Requests do not share common prefixes (system prompts, few-shot examples)
- Cache eviction too aggressive for the workload

## Recommendations

- Enable prefix caching: add `--enable-prefix-caching` to vLLM startup
- Ensure requests share a common system prompt or few-shot prefix
- Review `prefix_caching_hash_algo` if cache collisions are suspected

## Metrics used

- `vllm:prefix_cache_hits_total`
- `vllm:prefix_cache_queries_total`

## Configuration

| Setting      | Default |
| ------------ | ------- |
| Min hit rate | `0.50`  |

CLI flags for threshold overrides are planned.
