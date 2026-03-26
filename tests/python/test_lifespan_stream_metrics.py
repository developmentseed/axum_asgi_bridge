from __future__ import annotations

from axum_asgi_bridge import AxumAsgiApp, install_lifespan
from axum_asgi_bridge._native import demo_app as _demo_native
from fastapi import FastAPI


class _MockNative:
    def __init__(self) -> None:
        self.started = False
        self.stopped = False

    async def dispatch(self, method, path, query_string, headers, body):
        return 200, [("content-type", "text/plain")], b"abcdefgh"

    async def on_startup(self):
        self.started = True

    async def on_shutdown(self):
        self.stopped = True


class _MockStreamingNative(_MockNative):
    async def dispatch_streaming(self, method, path, query_string, headers, body):
        return 200, [("content-type", "text/plain")], [b"ab", b"cd", b"ef"]


class _MockWebSocketNative(_MockNative):
    def __init__(self) -> None:
        super().__init__()
        self.websocket_called = False

    async def dispatch_websocket(self, scope, receive, send):
        self.websocket_called = True


class _MockDispatchToSendNative(_MockNative):
    def __init__(self) -> None:
        super().__init__()
        self.called = False

    async def dispatch_to_send(self, method, path, query_string, headers, body, send):
        self.called = True
        await send({"type": "http.response.start", "status": 200, "headers": []})
        await send({"type": "http.response.body", "body": b"hello", "more_body": True})
        await send({"type": "http.response.body", "body": b" world", "more_body": False})


async def test_asgi_lifespan_events_are_handled() -> None:
    native = _MockNative()
    app = AxumAsgiApp(native)

    events = iter(
        [
            {"type": "lifespan.startup"},
            {"type": "lifespan.shutdown"},
        ]
    )
    sent = []

    async def receive():
        return next(events)

    async def send(event):
        sent.append(event)

    await app({"type": "lifespan"}, receive, send)

    assert native.started is True
    assert native.stopped is True
    assert sent[0]["type"] == "lifespan.startup.complete"
    assert sent[1]["type"] == "lifespan.shutdown.complete"


async def test_chunked_streaming_emits_multiple_body_events() -> None:
    native = _MockNative()
    app = AxumAsgiApp(native, stream_chunk_size=3)
    sent = []

    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def send(event):
        sent.append(event)

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/",
        "query_string": b"",
        "headers": [],
    }
    await app(scope, receive, send)

    assert sent[0]["type"] == "http.response.start"
    body_events = [event for event in sent if event["type"] == "http.response.body"]
    assert len(body_events) == 3
    assert body_events[0]["more_body"] is True
    assert body_events[-1]["more_body"] is False


async def test_request_done_hook_receives_metrics_payload() -> None:
    native = _MockNative()
    captured = {}

    def on_request_done(**kwargs):
        captured.update(kwargs)

    app = AxumAsgiApp(native, on_request_done=on_request_done)

    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def send(_event):
        return None

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/metrics",
        "query_string": b"",
        "headers": [],
    }
    await app(scope, receive, send)

    assert captured["method"] == "GET"
    assert captured["path"] == "/metrics"
    assert captured["status"] == 200
    assert captured["response_bytes"] == 8
    assert captured["duration_s"] >= 0


async def test_native_streaming_path_preserves_chunk_boundaries() -> None:
    native = _MockStreamingNative()
    app = AxumAsgiApp(native)
    sent = []

    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def send(event):
        sent.append(event)

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/",
        "query_string": b"",
        "headers": [],
    }
    await app(scope, receive, send)

    body_events = [event for event in sent if event["type"] == "http.response.body"]
    assert [event["body"] for event in body_events] == [b"ab", b"cd", b"ef"]


async def test_websocket_calls_native_dispatch_when_available() -> None:
    native = _MockWebSocketNative()
    app = AxumAsgiApp(native)

    async def receive():
        return {"type": "websocket.connect"}

    async def send(_event):
        return None

    await app({"type": "websocket", "path": "/ws"}, receive, send)
    assert native.websocket_called is True


async def test_install_lifespan_invokes_native_hooks() -> None:
    native = _MockNative()
    delegated = AxumAsgiApp(native)
    app = FastAPI()

    install_lifespan(app, delegated)

    async with app.router.lifespan_context(app):
        assert native.started is True

    assert native.stopped is True


async def test_http_prefers_native_dispatch_to_send_for_backpressure() -> None:
    native = _MockDispatchToSendNative()
    captured = {}

    def on_request_done(**kwargs):
        captured.update(kwargs)

    app = AxumAsgiApp(native, on_request_done=on_request_done)
    sent = []

    async def receive():
        return {"type": "http.request", "body": b"", "more_body": False}

    async def send(event):
        sent.append(event)

    scope = {
        "type": "http",
        "method": "GET",
        "path": "/",
        "query_string": b"",
        "headers": [],
    }
    await app(scope, receive, send)

    assert native.called is True
    body_events = [event for event in sent if event["type"] == "http.response.body"]
    assert len(body_events) == 2
    assert body_events[0]["more_body"] is True
    assert body_events[-1]["more_body"] is False
    assert captured["status"] == 200
    assert captured["response_bytes"] == len(b"hello world")


async def test_native_websocket_protocol_loop_echoes_text() -> None:
    native = _demo_native()

    events = iter(
        [
            {"type": "websocket.connect"},
            {"type": "websocket.receive", "text": "ping", "bytes": None},
            {"type": "websocket.disconnect", "code": 1000},
        ]
    )
    sent = []

    async def receive():
        return next(events)

    async def send(event):
        sent.append(event)

    scope = {"type": "websocket", "path": "/ws"}
    await native.dispatch_websocket(scope, receive, send)

    assert sent[0]["type"] == "websocket.accept"
    assert sent[1]["type"] == "websocket.send"
    assert sent[1]["text"] == "ping"
