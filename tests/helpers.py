import json
from pathlib import Path

import httpx

from vllm_doctor.clients.prometheus import PrometheusClient
from vllm_doctor.clients.scrape import ScrapeClient
from vllm_doctor.collector import collect
from vllm_doctor.models import Metrics

_SCRAPE_FIXTURES = Path(__file__).parent / "fixtures" / "scrape"
_PROMETHEUS_FIXTURES = Path(__file__).parent / "fixtures" / "prometheus"


async def snapshot_from_scrape_fixture(name: str) -> Metrics:
    text = (_SCRAPE_FIXTURES / name).read_text()
    transport = httpx.MockTransport(lambda _: httpx.Response(200, text=text, headers={"content-type": "text/plain"}))
    async with ScrapeClient(
        url="http://testserver/metrics",
        client=httpx.AsyncClient(transport=transport),
    ) as client:
        return await collect(client, window="now")


async def snapshot_from_prometheus_fixture(name: str, window: str = "1h") -> Metrics:
    metrics: dict[str, float] = json.loads((_PROMETHEUS_FIXTURES / name).read_text())

    def handler(r: httpx.Request) -> httpx.Response:
        query = r.url.params.get("query", "")
        value = next((v for k, v in metrics.items() if k in query), None)
        result = [{"metric": {}, "value": [1234567890, str(value)]}] if value is not None else []
        body = {"status": "success", "data": {"resultType": "vector", "result": result}}
        return httpx.Response(200, json=body)

    async with PrometheusClient(
        base_url="http://testserver",
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    ) as client:
        return await collect(client, window=window)
