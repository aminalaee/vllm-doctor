all: lint test

setup:
	uv sync --all-groups

test:
	uv run pytest tests/unit --cov=vllm_doctor --cov-report=term-missing --cov-fail-under=90

test-integration:
	uv run pytest tests/integration/ -v

lint:
	uv run ruff check vllm_doctor tests
	uv run ruff format --check vllm_doctor tests

format:
	uv run ruff format vllm_doctor tests
	uv run ruff check --fix vllm_doctor tests

docs:
	uv run zensical serve

docs-build:
	uv run zensical build

build:
	uv build

publish:
	uv publish

.PHONY: all setup test test-integration lint format docs docs-build build publish
