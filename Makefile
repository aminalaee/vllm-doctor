all: lint test

setup:
	uv sync --all-groups

test:
	uv run pytest --cov=vllm_doctor --cov-report=term-missing

lint:
	uv run ruff check vllm_doctor tests
	uv run ruff format --check vllm_doctor tests

format:
	uv run ruff format vllm_doctor tests
	uv run ruff check --fix vllm_doctor tests

build:
	uv build

.PHONY: all setup test lint format build
