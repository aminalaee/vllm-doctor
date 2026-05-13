from vllm_doctor.prometheus import PrometheusClient


def _sum_values(result: list[dict]) -> float | None:
    if not result:
        return None
    return sum(float(series["value"][1]) for series in result)


async def query_requests_running(
    client: PrometheusClient, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:num_requests_running{label}"))


async def query_requests_waiting(
    client: PrometheusClient, model: str | None = None
) -> float | None:
    label = f'{{model_name="{model}"}}' if model else ""
    return _sum_values(await client.query(f"vllm:num_requests_waiting{label}"))
