from __future__ import annotations

import json
from time import perf_counter
from typing import Any


class AxumAsgiApp:
    """Python ASGI adapter for a native bridge object.

    The native object must expose:
    1. ``dispatch(method, path, query_string, headers, body)``
       → awaitable returning ``(status, headers_list, body)``
    2. ``openapi_schema_json() -> Optional[str]``
    3. ``provided_route_patterns_json() -> str``

    The ``dispatch`` path avoids all JSON serialization, passing structured
    Python objects directly into Rust via PyO3 for maximum throughput.
    """

    __slots__ = ("_native", "_on_request_done", "_stream_chunk_size")

    def __init__(
        self,
        native_app: Any,
        *,
        on_request_done: Any | None = None,
        stream_chunk_size: int = 0,
    ):
        self._native = native_app
        self._on_request_done = on_request_done
        self._stream_chunk_size = max(0, int(stream_chunk_size))

    async def __call__(self, scope: dict[str, Any], receive: Any, send: Any) -> None:
        scope_type = scope.get("type")
        if scope_type == "lifespan":
            await self._handle_lifespan(receive, send)
            return
        if scope_type == "websocket":
            if hasattr(self._native, "dispatch_websocket"):
                await self._native.dispatch_websocket(scope, receive, send)
                return
            raise NotImplementedError("websocket scope is not implemented by this bridge")
        if scope_type != "http":
            raise NotImplementedError(f"unsupported ASGI scope type: {scope_type}")

        body_chunks: list[bytes] = []
        while True:
            event = await receive()
            if event.get("type") != "http.request":
                continue
            chunk = event.get("body", b"") or b""
            if isinstance(chunk, str):
                chunk = chunk.encode("utf-8")
            body_chunks.append(chunk)
            if not event.get("more_body", False):
                break

        query_string = scope.get("query_string", b"")
        if isinstance(query_string, bytes):
            query_string = query_string.decode("utf-8")

        headers = [
            (
                k.decode("utf-8") if isinstance(k, bytes) else k,
                v.decode("utf-8") if isinstance(v, bytes) else v,
            )
            for k, v in scope.get("headers", [])
        ]

        start = perf_counter()
        status: int
        response_headers: list[tuple[str, str]]
        stream_chunks: list[bytes] | None = None

        if hasattr(self._native, "dispatch_streaming"):
            status, response_headers, stream_chunks = await self._native.dispatch_streaming(
                scope.get("method", "GET"),
                scope.get("path", "/"),
                query_string,
                headers,
                b"".join(body_chunks),
            )
            response_body = b"".join(stream_chunks)
        else:
            status, response_headers, response_body = await self._native.dispatch(
                scope.get("method", "GET"),
                scope.get("path", "/"),
                query_string,
                headers,
                b"".join(body_chunks),
            )
        elapsed = perf_counter() - start

        if self._on_request_done is not None:
            self._on_request_done(
                method=scope.get("method", "GET"),
                path=scope.get("path", "/"),
                status=status,
                duration_s=elapsed,
                response_bytes=len(response_body),
            )

        encoded_headers = [
            (
                name.encode("utf-8") if isinstance(name, str) else name,
                value.encode("utf-8") if isinstance(value, str) else value,
            )
            for name, value in response_headers
        ]
        await send({"type": "http.response.start", "status": status, "headers": encoded_headers})

        if stream_chunks is not None and self._stream_chunk_size <= 0:
            if not stream_chunks:
                await send({"type": "http.response.body", "body": b"", "more_body": False})
                return

            for index, chunk in enumerate(stream_chunks):
                await send(
                    {
                        "type": "http.response.body",
                        "body": chunk,
                        "more_body": index < len(stream_chunks) - 1,
                    }
                )
            return

        if self._stream_chunk_size <= 0 or len(response_body) <= self._stream_chunk_size:
            await send({"type": "http.response.body", "body": response_body, "more_body": False})
            return

        index = 0
        total = len(response_body)
        while index < total:
            chunk = response_body[index : index + self._stream_chunk_size]
            index += self._stream_chunk_size
            await send(
                {
                    "type": "http.response.body",
                    "body": chunk,
                    "more_body": index < total,
                }
            )

    async def _handle_lifespan(self, receive: Any, send: Any) -> None:
        while True:
            event = await receive()
            event_type = event.get("type")
            if event_type == "lifespan.startup":
                try:
                    if hasattr(self._native, "on_startup"):
                        await self._native.on_startup()
                    await send({"type": "lifespan.startup.complete"})
                except Exception as exc:
                    await send({"type": "lifespan.startup.failed", "message": str(exc)})
                    return
            elif event_type == "lifespan.shutdown":
                try:
                    if hasattr(self._native, "on_shutdown"):
                        await self._native.on_shutdown()
                    await send({"type": "lifespan.shutdown.complete"})
                except Exception as exc:
                    await send({"type": "lifespan.shutdown.failed", "message": str(exc)})
                return

    def openapi_schema(self) -> dict[str, Any] | None:
        raw = self._native.openapi_schema_json()
        if raw is None:
            return None
        return json.loads(raw)

    def provided_route_patterns(self) -> list[str]:
        return json.loads(self._native.provided_route_patterns_json())
