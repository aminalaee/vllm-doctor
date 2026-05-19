# Queue Latency

Detects when requests are spending too long in the waiting queue before prefill begins.

## Background

`vllm:request_queue_time_seconds` measures the time a request spends in the WAITING phase — from arrival to when prefill starts. Unlike the waiting request count, this histogram gives a direct latency measurement: clients experience this delay before receiving any output.

High p95 queue time means the server cannot admit requests fast enough, even if the queue depth looks moderate.

!!! note "Prometheus mode only"
    Queue time percentiles require `histogram_quantile()` over a time window. This rule does not fire in direct scrape mode.

## Signals

| Signal          | Condition                                           |
| --------------- | --------------------------------------------------- |
| High queue time | `queue_time_p95_seconds >= 1.0` (default threshold) |
| Active backlog  | `num_requests_waiting > 0` — backlog confirmed      |

## Confidence

| Signals matched                    | Confidence |
| ---------------------------------- | ---------- |
| High queue time only               | Low        |
| High queue time + requests waiting | High       |

## Likely causes

- Insufficient replica capacity for current request rate
- Long-context requests blocking admission of new sequences
- Autoscaling has not reacted to traffic increase
- KV cache exhaustion limiting sequence admission

## Recommendations

- Add replicas or increase concurrency limits
- Inspect autoscaling thresholds and reaction time
- Correlate with KV cache pressure — reduce `max_num_seqs` if cache is full
- Separate long-context traffic to a dedicated replica

## Metrics used

- `vllm:request_queue_time_seconds` (histogram)
- `vllm:num_requests_waiting`

## Configuration

| Setting                  | Default |
| ------------------------ | ------- |
| Queue time p95 threshold | `1.0s`  |
