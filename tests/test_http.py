import asyncio
import contextlib
import logging
import socket
import threading
import time
import httpx

import pytest

from tests.response import Response
from tests.utils import run_server

from quanshu import serve
from quanshu.config import Config

SIMPLE_GET_REQUEST = b"\r\n".join([b"GET / HTTP/1.1", b"Host: example.org", b"", b""])

SIMPLE_HEAD_REQUEST = b"\r\n".join([b"HEAD / HTTP/1.1", b"Host: example.org", b"", b""])

SIMPLE_POST_REQUEST = b"\r\n".join(
    [
        b"POST / HTTP/1.1",
        b"Host: example.org",
        b"Content-Type: application/json",
        b"Content-Length: 18",
        b"",
        b'{"hello": "world"}',
    ]
)

LARGE_POST_REQUEST = b"\r\n".join(
    [
        b"POST / HTTP/1.1",
        b"Host: example.org",
        b"Content-Type: text/plain",
        b"Content-Length: 100000",
        b"",
        b"x" * 100000,
    ]
)

START_POST_REQUEST = b"\r\n".join(
    [
        b"POST / HTTP/1.1",
        b"Host: example.org",
        b"Content-Type: application/json",
        b"Content-Length: 18",
        b"",
        b"",
    ]
)

FINISH_POST_REQUEST = b'{"hello": "world"}'

HTTP10_GET_REQUEST = b"\r\n".join([b"GET / HTTP/1.0", b"Host: example.org", b"", b""])

GET_REQUEST_WITH_RAW_PATH = b"\r\n".join(
    [b"GET /one%2Ftwo HTTP/1.1", b"Host: example.org", b"", b""]
)

UPGRADE_REQUEST = b"\r\n".join(
    [
        b"GET / HTTP/1.1",
        b"Host: example.org",
        b"Connection: upgrade",
        b"Upgrade: websocket",
        b"Sec-WebSocket-Version: 11",
        b"",
        b"",
    ]
)

INVALID_REQUEST_TEMPLATE = b"\r\n".join(
    [
        b"%s",
        b"Host: example.org",
        b"",
        b"",
    ]
)

@pytest.mark.asyncio
async def test_get_request():
    app = Response("Hello, world", media_type="text/plain")
    config = Config(app)

    async with run_server(config):
        async with httpx.AsyncClient() as client:
            response = await client.get("http://127.0.0.1:8000")
            assert response.http_version == "HTTP/1.1"
            assert response.reason_phrase == "OK"
            assert response.content == b"Hello, world"
            assert response.status_code == 200
        #     assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        # assert b"Hello, world" in protocol.transport.buffer

'''
@pytest.mark.parametrize("path", ["/", "/?foo", "/?foo=bar", "/?foo=bar&baz=1"])
def test_request_logging(path, caplog, event_loop):
    get_request_with_query_string = b"\r\n".join(
        ["GET {} HTTP/1.1".format(path).encode("ascii"), b"Host: example.org", b"", b""]
    )
    caplog.set_level(logging.INFO, logger="quanshu.access")
    logging.getLogger("quanshu.access").propagate = True

    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(
        app, protocol_cls, event_loop, log_config=None
    ) as protocol:
        protocol.data_received(get_request_with_query_string)
        protocol.loop.run_one()
        assert '"GET {} HTTP/1.1" 200'.format(path) in caplog.records[0].message


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_head_request(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_HEAD_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"Hello, world" not in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_post_request(protocol_cls, event_loop):
    async def app(scope, receive, send):
        body = b""
        more_body = True
        while more_body:
            message = await receive()
            body += message.get("body", b"")
            more_body = message.get("more_body", False)
        response = Response(b"Body: " + body, media_type="text/plain")
        await response(scope, receive, send)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_POST_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b'Body: {"hello": "world"}' in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_keepalive(protocol_cls, event_loop):
    app = Response(b"", status_code=204)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()

        assert b"HTTP/1.1 204 No Content" in protocol.transport.buffer
        assert not protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_keepalive_timeout(protocol_cls, event_loop):
    app = Response(b"", status_code=204)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 204 No Content" in protocol.transport.buffer
        assert not protocol.transport.is_closing()
        protocol.loop.run_later(with_delay=1)
        assert not protocol.transport.is_closing()
        protocol.loop.run_later(with_delay=5)
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_close(protocol_cls, event_loop):
    app = Response(b"", status_code=204, headers={"connection": "close"})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 204 No Content" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_chunked_encoding(protocol_cls, event_loop):
    app = Response(
        b"Hello, world!", status_code=200, headers={"transfer-encoding": "chunked"}
    )

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"0\r\n\r\n" in protocol.transport.buffer
        assert not protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_chunked_encoding_empty_body(protocol_cls, event_loop):
    app = Response(
        b"Hello, world!", status_code=200, headers={"transfer-encoding": "chunked"}
    )

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert protocol.transport.buffer.count(b"0\r\n\r\n") == 1
        assert not protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_chunked_encoding_head_request(protocol_cls, event_loop):
    app = Response(
        b"Hello, world!", status_code=200, headers={"transfer-encoding": "chunked"}
    )

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_HEAD_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert not protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_pipelined_requests(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"Hello, world" in protocol.transport.buffer
        protocol.transport.clear_buffer()
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"Hello, world" in protocol.transport.buffer
        protocol.transport.clear_buffer()
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"Hello, world" in protocol.transport.buffer
        protocol.transport.clear_buffer()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_undersized_request(protocol_cls, event_loop):
    app = Response(b"xxx", headers={"content-length": "10"})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_oversized_request(protocol_cls, event_loop):
    app = Response(b"xxx" * 20, headers={"content-length": "10"})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_large_post_request(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(LARGE_POST_REQUEST)
        assert protocol.transport.read_paused
        protocol.loop.run_one()
        assert not protocol.transport.read_paused


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_invalid_http(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(b"x" * 100000)
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_app_exception(protocol_cls, event_loop):
    async def app(scope, receive, send):
        raise Exception()

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 500 Internal Server Error" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_exception_during_response(protocol_cls, event_loop):
    async def app(scope, receive, send):
        await send({"type": "http.response.start", "status": 200})
        await send({"type": "http.response.body", "body": b"1", "more_body": True})
        raise Exception()

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 500 Internal Server Error" not in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_no_response_returned(protocol_cls, event_loop):
    async def app(scope, receive, send):
        pass

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 500 Internal Server Error" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_partial_response_returned(protocol_cls, event_loop):
    async def app(scope, receive, send):
        await send({"type": "http.response.start", "status": 200})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 500 Internal Server Error" not in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_duplicate_start_message(protocol_cls, event_loop):
    async def app(scope, receive, send):
        await send({"type": "http.response.start", "status": 200})
        await send({"type": "http.response.start", "status": 200})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 500 Internal Server Error" not in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_missing_start_message(protocol_cls, event_loop):
    async def app(scope, receive, send):
        await send({"type": "http.response.body", "body": b""})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 500 Internal Server Error" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_message_after_body_complete(protocol_cls, event_loop):
    async def app(scope, receive, send):
        await send({"type": "http.response.start", "status": 200})
        await send({"type": "http.response.body", "body": b""})
        await send({"type": "http.response.body", "body": b""})

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_value_returned(protocol_cls, event_loop):
    async def app(scope, receive, send):
        await send({"type": "http.response.start", "status": 200})
        await send({"type": "http.response.body", "body": b""})
        return 123

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_early_disconnect(protocol_cls, event_loop):
    got_disconnect_event = False

    async def app(scope, receive, send):
        nonlocal got_disconnect_event

        while True:
            message = await receive()
            if message["type"] == "http.disconnect":
                break

        got_disconnect_event = True

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_POST_REQUEST)
        protocol.eof_received()
        protocol.connection_lost(None)
        protocol.loop.run_one()
        assert got_disconnect_event


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_early_response(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(START_POST_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        protocol.data_received(FINISH_POST_REQUEST)
        assert not protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_read_after_response(protocol_cls, event_loop):
    message_after_response = None

    async def app(scope, receive, send):
        nonlocal message_after_response

        response = Response("Hello, world", media_type="text/plain")
        await response(scope, receive, send)
        message_after_response = await receive()

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_POST_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert message_after_response == {"type": "http.disconnect"}


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_http10_request(protocol_cls, event_loop):
    async def app(scope, receive, send):
        content = "Version: %s" % scope["http_version"]
        response = Response(content, media_type="text/plain")
        await response(scope, receive, send)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(HTTP10_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"Version: 1.0" in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_root_path(protocol_cls, event_loop):
    async def app(scope, receive, send):
        path = scope.get("root_path", "") + scope["path"]
        response = Response("Path: " + path, media_type="text/plain")
        await response(scope, receive, send)

    with get_connected_protocol(
        app, protocol_cls, event_loop, root_path="/app"
    ) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b"Path: /app/" in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_raw_path(protocol_cls, event_loop):
    async def app(scope, receive, send):
        path = scope["path"]
        raw_path = scope.get("raw_path", None)
        assert "/one/two" == path
        assert b"/one%2Ftwo" == raw_path

        response = Response("Done", media_type="text/plain")
        await response(scope, receive, send)

    with get_connected_protocol(
        app, protocol_cls, event_loop, root_path="/app"
    ) as protocol:
        protocol.data_received(GET_REQUEST_WITH_RAW_PATH)
        protocol.loop.run_one()
        assert b"Done" in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_max_concurrency(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(
        app, protocol_cls, event_loop, limit_concurrency=1
    ) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 503 Service Unavailable" in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_shutdown_during_request(protocol_cls, event_loop):
    app = Response(b"", status_code=204)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.shutdown()
        protocol.loop.run_one()
        assert b"HTTP/1.1 204 No Content" in protocol.transport.buffer
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_shutdown_during_idle(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.shutdown()
        assert protocol.transport.buffer == b""
        assert protocol.transport.is_closing()


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_100_continue_sent_when_body_consumed(protocol_cls, event_loop):
    async def app(scope, receive, send):
        body = b""
        more_body = True
        while more_body:
            message = await receive()
            body += message.get("body", b"")
            more_body = message.get("more_body", False)
        response = Response(b"Body: " + body, media_type="text/plain")
        await response(scope, receive, send)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        EXPECT_100_REQUEST = b"\r\n".join(
            [
                b"POST / HTTP/1.1",
                b"Host: example.org",
                b"Expect: 100-continue",
                b"Content-Type: application/json",
                b"Content-Length: 18",
                b"",
                b'{"hello": "world"}',
            ]
        )
        protocol.data_received(EXPECT_100_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 100 Continue" in protocol.transport.buffer
        assert b"HTTP/1.1 200 OK" in protocol.transport.buffer
        assert b'Body: {"hello": "world"}' in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_100_continue_not_sent_when_body_not_consumed(protocol_cls, event_loop):
    app = Response(b"", status_code=204)

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        EXPECT_100_REQUEST = b"\r\n".join(
            [
                b"POST / HTTP/1.1",
                b"Host: example.org",
                b"Expect: 100-continue",
                b"Content-Type: application/json",
                b"Content-Length: 18",
                b"",
                b'{"hello": "world"}',
            ]
        )
        protocol.data_received(EXPECT_100_REQUEST)
        protocol.loop.run_one()
        assert b"HTTP/1.1 100 Continue" not in protocol.transport.buffer
        assert b"HTTP/1.1 204 No Content" in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_unsupported_upgrade_request(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(app, protocol_cls, event_loop, ws="none") as protocol:
        protocol.data_received(UPGRADE_REQUEST)
        assert b"HTTP/1.1 400 Bad Request" in protocol.transport.buffer
        assert b"Unsupported upgrade request." in protocol.transport.buffer


@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_supported_upgrade_request(protocol_cls, event_loop):
    app = Response("Hello, world", media_type="text/plain")

    with get_connected_protocol(
        app, protocol_cls, event_loop, ws="wsproto"
    ) as protocol:
        protocol.data_received(UPGRADE_REQUEST)
        assert b"HTTP/1.1 426 " in protocol.transport.buffer


async def asgi3app(scope, receive, send):
    pass


def asgi2app(scope):
    async def asgi(receive, send):
        pass

    return asgi


asgi_scope_data = [
    (asgi3app, {"version": "3.0", "spec_version": "2.3"}),
    (asgi2app, {"version": "2.0", "spec_version": "2.3"}),
]


@pytest.mark.parametrize("asgi2or3_app, expected_scopes", asgi_scope_data)
@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_scopes(asgi2or3_app, expected_scopes, protocol_cls, event_loop):
    with get_connected_protocol(asgi2or3_app, protocol_cls, event_loop) as protocol:
        protocol.data_received(SIMPLE_GET_REQUEST)
        protocol.loop.run_one()
        assert expected_scopes == protocol.scope.get("asgi")


@pytest.mark.parametrize(
    "request_line",
    [
        pytest.param(b"G?T / HTTP/1.1", id="invalid-method"),
        pytest.param(b"GET /?x=y z HTTP/1.1", id="invalid-path"),
        pytest.param(b"GET / HTTP1.1", id="invalid-http-version"),
    ],
)
@pytest.mark.parametrize("protocol_cls", HTTP_PROTOCOLS)
def test_invalid_http_request(request_line, protocol_cls, caplog, event_loop):
    app = Response("Hello, world", media_type="text/plain")
    request = INVALID_REQUEST_TEMPLATE % request_line

    caplog.set_level(logging.INFO, logger="quanshu.error")
    logging.getLogger("quanshu.error").propagate = True

    with get_connected_protocol(app, protocol_cls, event_loop) as protocol:
        protocol.data_received(request)
        assert b"HTTP/1.1 400 Bad Request" in protocol.transport.buffer
        assert b"Invalid HTTP request received." in protocol.transport.buffer


def test_fragmentation():
    def receive_all(sock):
        chunks = []
        while True:
            chunk = sock.recv(1024)
            if not chunk:
                break
            chunks.append(chunk)
        return b"".join(chunks)

    app = Response("Hello, world", media_type="text/plain")

    def send_fragmented_req(path):

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect(("127.0.0.1", 8000))
        d = (
            f"GET {path} HTTP/1.1\r\n" "Host: localhost\r\n" "Connection: close\r\n\r\n"
        ).encode()
        split = len(path) // 2
        sock.sendall(d[:split])
        time.sleep(0.01)
        sock.sendall(d[split:])
        resp = receive_all(sock)
        # see https://github.com/kmonsoor/py-amqplib/issues/45
        # we skip the error on bsd systems if python is too slow
        try:
            sock.shutdown(socket.SHUT_RDWR)
        except Exception:
            pass
        sock.close()
        return resp

    config = Config(app=app, http="httptools")
    server = Server(config=config)
    t = threading.Thread(target=server.run)
    t.daemon = True
    t.start()
    time.sleep(1)  # wait for quanshu to start

    path = "/?param=" + "q" * 10
    response = send_fragmented_req(path)
    bad_response = b"HTTP/1.1 400 Bad Request"
    assert bad_response != response[: len(bad_response)]
    server.should_exit = True
    t.join()
'''
