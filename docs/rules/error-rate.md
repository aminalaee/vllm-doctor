# Error Rate

Detects elevated server-side errors or client aborts relative to total requests.

## Background

vLLM tracks completed requests by `finished_reason`:

| Reason       | Meaning                                   |
| ------------ | ----------------------------------------- |
| `stop`       | Completed normally                        |
| `error`      | Server-side failure (OOM, internal error) |
| `abort`      | Client disconnected or request cancelled  |
| `length`     | Hit `max_tokens` limit                    |
| `repetition` | Stopped by repetition penalty             |

This rule monitors `error` and `abort` rates. A high `error` rate indicates the server is failing requests internally. A high `abort` rate often means clients are giving up — typically because responses are too slow.

## Signals

| Signal          | Condition                          |
| --------------- | ---------------------------------- |
| Error rate high | `errors / total >= 0.05` (default) |
| Abort rate high | `aborts / total >= 0.10` (default) |

## Confidence

| Signals matched         | Confidence |
| ----------------------- | ---------- |
| Error high only         | Low        |
| Abort high only         | Low        |
| Both error + abort high | High       |

## Severity

- **Critical** when error rate is high — server is actively failing requests
- **Warning** when only abort rate is high — clients are disconnecting

## Likely causes

- Server-side OOM or internal errors under high load
- Requests exceeding timeout limits causing client aborts
- High latency causing clients to disconnect before completion
- Resource exhaustion correlating with KV cache pressure

## Recommendations

- Inspect vLLM server logs for error details
- Correlate with KV cache pressure and queue pressure findings
- Check client timeout settings relative to observed TTFT and TPOT
- Reduce load or add replicas if errors correlate with traffic spikes

## Metrics used

- `vllm:request_success_total{finished_reason="error"}`
- `vllm:request_success_total{finished_reason="abort"}`
- `vllm:request_success_total{finished_reason="stop"}`

## Configuration

```toml
[rules.error_rate]
high_error_rate = 0.02   # default: 0.05
high_abort_rate = 0.05   # default: 0.10
```
