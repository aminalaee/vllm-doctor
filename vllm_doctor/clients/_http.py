import httpx

from vllm_doctor.clients.exceptions import ClientConnectionError, ClientError


async def _get(client: httpx.AsyncClient, url: str, params: dict[str, str] | None = None) -> httpx.Response:
    try:
        response = await client.get(url, params=params)
        response.raise_for_status()
        return response
    except httpx.ConnectError as e:
        raise ClientConnectionError(str(e)) from e
    except httpx.HTTPStatusError as e:
        raise ClientError(str(e)) from e
