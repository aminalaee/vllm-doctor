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
  Requests are likely waiting because the server has limited KV cache
  headroom, often caused by high concurrency or long-context requests.
```

The same summary appears as a top-level `assessment` object in `--output json`
(see [JSON Output](json-output.md#assessment)).

## Categories

The assessment classifies the run into one of:

| Category            | Meaning                                                         |
| ------------------- | --------------------------------------------------------------- |
| Queue saturation    | Requests arrive faster than they can be served; the queue grows |
| KV cache saturation | Requests wait on limited KV cache headroom                      |
| Long prefill        | High TTFT with no queue — long input prompts dominate prefill   |
| Decode / TPOT       | Per-token generation is the bottleneck, not queueing            |
| Replica imbalance   | Load is unevenly distributed across replicas                    |
| Error or failure    | The server is returning errors or aborting requests             |
| Idle                | No active traffic; throughput/latency warnings are suppressed   |
| No clear bottleneck | Evidence is weak or conflicting                                 |

## How to read it

The assessment summarizes the findings and measurements from the current run.
Its evidence contains the observed values supporting the diagnosis, and its
confidence indicates how strongly those values point to the reported
bottleneck.

When evidence is weak or conflicting, vLLM Doctor reports **No clear
bottleneck** or low confidence rather than guessing.

!!! note "Sequence-length evidence is optional"
    Distinguishing long-prefill from decode-heavy workloads is sharper when
    vLLM's prompt/generation token histograms
    (`vllm:request_prompt_tokens_bucket`, `vllm:request_generation_tokens_bucket`)
    are available. When they are not exposed, the assessment still works — it
    simply relies on latency and queue signals and does not use prompt/output
    length evidence.
