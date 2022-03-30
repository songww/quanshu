import socket
import logging

import httpx
import pytest
from quanshu import serve

from quanshu.config import Config
from tests.utils import run_server

async def app(scope, receive, send):
    assert scope["type"] == "http"
    await send({"type": "http.response.start", "status": 204, "headers": []})
    await send({"type": "http.response.body", "body": b"", "more_body": False})


@pytest.mark.asyncio
async def test_return_close_header():
    config = Config(app=app, host="localhost", loop="asyncio", limit_max_requests=1)
    async with run_server(config):
        async with httpx.AsyncClient() as client:
            response = await client.get(
                "http://127.0.0.1:8000", headers={"connection": "close"}
            )

    assert response.status_code == 204
    assert (
        "connection" in response.headers and response.headers["connection"] == "close"
    )

def _has_ipv6(host):
    sock = None
    has_ipv6 = False
    if socket.has_ipv6:
        try:
            sock = socket.socket(socket.AF_INET6)
            sock.bind((host, 0))
            has_ipv6 = True
        except Exception:
            pass
    if sock:
        sock.close()
    return has_ipv6


@pytest.mark.asyncio
@pytest.mark.parametrize(
    "host, url",
    [
        pytest.param("127.0.0.1", "http://127.0.0.1:8000", id="default"),
        pytest.param("localhost", "http://127.0.0.1:8000", id="hostname"),
        pytest.param(
            "::1",
            "http://[::1]:8000",
            id="ipv6",
            marks=pytest.mark.skipif(not _has_ipv6("::1"), reason="IPV6 not enabled"),
        ),
    ],
)
async def test_run(host, url):
    config = Config(app=app, host=host, loop="asyncio", limit_max_requests=1)
    async with run_server(config):
        async with httpx.AsyncClient() as client:
            response = await client.get(url)
    assert response.status_code == 204

@pytest.mark.asyncio
async def test_run_multiprocess():
    config = Config(app=app, loop="asyncio", workers=2, limit_max_requests=1)
    async with run_server(config):
        async with httpx.AsyncClient() as client:
            response = await client.get("http://127.0.0.1:8000")
    assert response.status_code == 204

@pytest.mark.skip("reload not supported yet.")
def test_run_invalid_app_config_combination(caplog: pytest.LogCaptureFixture) -> None:
    with pytest.raises(SystemExit) as exit_exception:
        #serve(app, reload=True)
        pass
    assert exit_exception.value.code == 1
    assert caplog.records[-1].name == "uvicorn.error"
    assert caplog.records[-1].levelno == logging.WARNING
    assert caplog.records[-1].message == (
        "You must pass the application as an import string to enable "
        "'reload' or 'workers'."
    )

@pytest.mark.skip("lifespan not supported yet.")
@pytest.mark.asyncio
async def test_run_startup_failure(caplog: pytest.LogCaptureFixture) -> None:
    async def app(scope, receive, send):
        assert scope["type"] == "lifespan"
        message = await receive()
        if message["type"] == "lifespan.startup":
            raise RuntimeError("Startup failed")

    with pytest.raises(SystemExit) as exit_exception:
        config = Config(app, lifespan=True)
        async with run_server(config):
            pass
    assert exit_exception.value.code == 3
