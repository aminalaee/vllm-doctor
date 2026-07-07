# syntax=docker/dockerfile:1

FROM rust:1.88-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
LABEL org.opencontainers.image.source="https://github.com/aminalaee/vllm-doctor"
LABEL org.opencontainers.image.description="Diagnostic tool for vLLM inference servers"

RUN apt-get update && apt-get install -y --no-install-recommends libssl3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 1000 vllm-doctor
COPY --from=build /src/target/release/vllm-doctor /usr/local/bin/vllm-doctor

USER vllm-doctor
ENTRYPOINT ["vllm-doctor"]