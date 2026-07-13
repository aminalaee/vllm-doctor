# Assessment

Every `diagnose` run ends with an **assessment** — a concise "Likely bottleneck"
summary that appears **before** the detailed findings. Where the findings list
individual symptoms, the assessment interprets the *combination* of them into a
single most-likely root cause, so you can start triage without reading every
rule.

The assessment never replaces or hides the findings — it sits on top of them.

## Example

```text
Likely bottleneck: KV cache saturation (high confidence)
  Cache is full (94%), so requests queue and TTFT climbs. The queue and TTFT
  findings are likely downstream — start with the KV cache.
```

The same summary appears as a top-level `assessment` object in `--output json`
(see [JSON Output](json-output.md#assessment)).

## Categories

The assessment classifies the run into one of:

| Category             | Meaning                                                        |
| -------------------- | -------------------------------------------------------------- |
| Queue saturation     | Requests arrive faster than they can be served; the queue grows |
| KV cache saturation  | Requests wait on limited KV cache headroom                     |
| Long prefill         | High TTFT with no queue — long input prompts dominate prefill  |
| Decode / TPOT        | Per-token generation is the bottleneck, not queueing           |
| Replica imbalance    | Load is unevenly distributed across replicas                   |
| Error or failure     | The server is returning errors or aborting requests            |
| Idle                 | No active traffic; throughput/latency warnings are suppressed  |
| No clear bottleneck  | Evidence is weak or conflicting                                |

## How it is derived

The assessment is a read-only interpretation of results the rules already
produced — it does not re-run any rule logic:

- **Classification** looks at which rules fired and, for a few cases, a raw
  metric value (e.g. whether any requests are waiting) to distinguish similar
  shapes such as long-prefill vs. queueing.
- **Evidence** is pulled from the findings that fired this run, so the bullets
  carry the run's real numbers rather than restating thresholds.
- **Confidence** starts from the primary finding's own confidence and rises as
  independent findings corroborate the same cause.

It is deliberately conservative: when evidence is weak or conflicting it returns
**No clear bottleneck** or low confidence rather than guessing. It also avoids
speculative claims (e.g. hardware choice) that the tool has no direct evidence
for.

!!! note "Sequence-length evidence is optional"
    Distinguishing long-prefill from decode-heavy workloads is sharper when
    vLLM's prompt/generation token histograms
    (`vllm:request_prompt_tokens_bucket`, `vllm:request_generation_tokens_bucket`)
    are available. When they are not exposed, the assessment still works — it
    simply relies on latency and queue signals and does not use prompt/output
    length evidence.
