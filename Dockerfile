# syntax=docker/dockerfile:1

# Build the wheel with uv.
FROM ghcr.io/astral-sh/uv:python3.14-bookworm-slim AS build
WORKDIR /src
COPY . .
RUN uv build --wheel --out-dir /dist

# Minimal runtime: install the wheel and its dependencies, run as non-root.
FROM python:3.14-slim
LABEL org.opencontainers.image.source="https://github.com/aminalaee/vllm-doctor"
LABEL org.opencontainers.image.description="Diagnostic tool for vLLM inference servers"

RUN useradd --create-home --uid 1000 vllm-doctor
COPY --from=build /dist/*.whl /tmp/
RUN pip install --no-cache-dir /tmp/*.whl && rm -rf /tmp/*.whl

USER vllm-doctor
ENTRYPOINT ["vllm-doctor"]
