# Replica Imbalance

Detects when load is unevenly distributed across the replicas of a deployment — one replica overloaded while its peers sit idle. This points at the routing layer rather than the model: an uneven load balancer, an unready replica receiving no traffic, or long-context requests pinned to a subset of pods.

Because one vLLM deployment serves one model, replicas are grouped by the `model_name` label and compared only against peers serving the same model. This keeps the comparison correct on a shared Prometheus that scrapes several deployments. A group with a single replica is skipped — there is nothing to compare.

## Signals

Each signal is evaluated per model group.

| Signal         | Condition                                                                                                                                                            |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Running spread | busiest replica handles `>= imbalance_factor x` the least busy (default `2x`), gated by `num_requests_running` summing to at least `min_total_running` (default `5`) |
| Cache gap      | `kv_cache_usage_perc` max − min `>= cache_gap` (default `0.30`)                                                                                                      |
| Waiting skew   | one replica has queued requests while another has none                                                                                                               |

## Confidence

| Signals matched | Confidence |
| --------------- | ---------- |
| 1               | Low        |
| 2               | Medium     |
| 3               | High       |

## Likely causes

- Load balancer not distributing requests evenly (sticky sessions or connection reuse)
- A replica is not Ready or recently restarted, so traffic skips it
- Long-context requests pinned to a subset of replicas
- Autoscaler added replicas that are not yet receiving traffic

## Recommendations

- Check the load balancer / service routing and session affinity settings
- Verify readiness probes — an unready replica receives no traffic
- Compare per-replica latency and restart any unhealthy replica
- Confirm newly added replicas are registered with the load balancer

## Metrics used

- `vllm:num_requests_running`
- `vllm:num_requests_waiting`
- `vllm:kv_cache_usage_perc`

## Configuration

```toml
[rules.replica_imbalance]
imbalance_factor = 2.0     # busiest / least-busy running ratio
cache_gap = 0.30           # kv cache usage max − min (fraction)
min_total_running = 5.0    # minimum total running load before the running signal fires
```

## Notes

- Requires per-replica labels in the metrics (e.g. `pod`, `instance`, `host`). In direct scrape mode against a single endpoint there is only one replica, so this rule does not fire — use a Prometheus target that scrapes all replicas.
- Latency imbalance (TTFT, TPOT, queue time) is not detected: those percentiles are returned as a single aggregate value, not per replica.
- Two separate deployments serving the **same** `model_name` (e.g. canary + prod) are grouped together and may report a false imbalance. Filter to one with a future `--model` flag or distinguish them by a deployment label.
