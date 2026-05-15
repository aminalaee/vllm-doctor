# Queue Pressure

Detects when requests are accumulating faster than the server can process them.

## Signals

| Signal                 | Condition                             |
| ---------------------- | ------------------------------------- |
| Waiting requests high  | `num_requests_waiting > 5` (default)  |
| Server near saturation | `num_requests_running > 50` (default) |

## Confidence

| Signals matched             | Confidence |
| --------------------------- | ---------- |
| Waiting high only           | Low        |
| Waiting high + running high | High       |

## Likely causes

- Insufficient replica capacity for current traffic
- Autoscaling has not reacted yet
- Long-context requests consuming disproportionate compute

## Recommendations

- Add replicas or increase concurrency limits
- Inspect autoscaling thresholds
- Separate long-context traffic to a dedicated replica
- Reduce incoming request rate

## Metrics used

- `vllm:num_requests_waiting`
- `vllm:num_requests_running`

## Configuration

These thresholds are currently fixed in the CLI:

| Setting                 | Default |
| ----------------------- | ------- |
| Queue waiting threshold | `5`     |
| Queue running threshold | `50`    |

CLI flags for threshold overrides are planned.
