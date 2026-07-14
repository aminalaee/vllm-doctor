# Examples

Use the mock example for a quick, deterministic tour of vLLM Doctor, or connect
the live example to a real local vLLM-Metal server.

| Example | Requires vLLM | Purpose |
| --- | --- | --- |
| [Mock metrics](mock/) | No | Exercise diagnoses with predefined Prometheus and `/metrics` scenarios. |
| [Live vLLM](live-vllm/) | vLLM-Metal | Run a small real model locally and diagnose its live metrics. |

The live example currently covers macOS with vLLM-Metal only. Other environments
can be added once they have a runnable, verified setup.
