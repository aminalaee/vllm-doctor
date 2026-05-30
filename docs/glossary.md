# Glossary

This glossary maps vLLM Doctor terms to common serving and benchmarking language.

## TTFT

**Time to first token** measures how long a client waits before receiving the first output token.

In vLLM Doctor, TTFT comes from `vllm:time_to_first_token_seconds`. High TTFT usually points to queue delay, long prefill work, or both.

## TPOT

**Time per output token** measures decode-token latency on the server side.

In vLLM Doctor, TPOT comes from `vllm:request_time_per_output_token_seconds`. High TPOT usually points to decode pressure, GPU memory bandwidth pressure, or too many concurrent sequences.

## ITL

**Inter-token latency** is a common client-side benchmarking term for the time between streamed output tokens.

TPOT and ITL describe neighboring ideas. vLLM Doctor uses TPOT because it diagnoses server-side vLLM metrics; tools such as GuideLLM may report the client-observed ITL for endpoint behavior.

## E2E Latency

**End-to-end latency** is the total client-observed time for a request, from submission until completion.

vLLM Doctor does not diagnose E2E latency directly today. It explains server-side contributors such as queue latency, TTFT, TPOT, cache pressure, and error or abort rates.

## Prefill

**Prefill** is the phase where vLLM processes the input prompt before generating output tokens.

Long prompts, low prefix-cache reuse, and queue pressure can make prefill-related latency worse. vLLM Doctor reports prefill throughput with `vllm:prompt_tokens_per_second`.

## Decode

**Decode** is the phase where vLLM generates output tokens.

Decode bottlenecks often show up as high TPOT or low decode throughput. vLLM Doctor reports decode throughput with `vllm:generation_tokens_per_second`.

## Queue Latency

**Queue latency** is the time a request spends waiting before prefill begins.

vLLM Doctor reads this from `vllm:request_queue_time_seconds` in Prometheus mode. High queue latency usually means the server cannot admit requests fast enough for the current load.

## KV Cache Pressure

**KV cache pressure** means GPU KV cache usage is high enough to limit admission or force preemptions.

When the cache is full, requests can wait even if raw GPU compute is not the only bottleneck.

## Prefix Cache Hit Rate

**Prefix cache hit rate** measures how often vLLM reuses cached prompt-prefix computation.

Low hit rate can increase redundant prefill work, especially for workloads with repeated system prompts, templates, or few-shot context.
