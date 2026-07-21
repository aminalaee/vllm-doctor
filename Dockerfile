# syntax=docker/dockerfile:1

FROM rust:1.88-alpine AS build
WORKDIR /src
RUN apk add --no-cache musl-dev
COPY . .
RUN cargo build --release

FROM alpine:3.22
LABEL org.opencontainers.image.source="https://github.com/vllm-doctor/vllm-doctor"
LABEL org.opencontainers.image.description="Diagnostic tool for vLLM inference servers"

RUN adduser --disabled-password --uid 1000 vllm-doctor
COPY --from=build /src/target/release/vllm-doctor /usr/local/bin/vllm-doctor

USER vllm-doctor
ENTRYPOINT ["vllm-doctor"]
