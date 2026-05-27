from pathlib import Path

import httpx

from vllm_doctor.clients.scrape import ScrapeClient
from vllm_doctor.collector import collect
from vllm_doctor.models import MetricSnapshot

_FIXTURES_DIR = Path(__file__).parent / "fixtures" / "metrics"


async def snapshot_from_fixture(name: str) -> MetricSnapshot:
    text = (_FIXTURES_DIR / name).read_text()
    transport = httpx.MockTransport(lambda _: httpx.Response(200, text=text, headers={"content-type": "text/plain"}))
    async with ScrapeClient(
        url="http://testserver/metrics",
        client=httpx.AsyncClient(transport=transport),
    ) as client:
        return await collect(client, window="now")
