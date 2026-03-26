from __future__ import annotations

import json
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

    __slots__ = ("_native",)

    def __init__(self, native_app: Any):
        self._native = native_app

    async def __call__(self, scope: dict[str, Any], receive: Any, send: Any) -> None:
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

        status, response_headers, response_body = await self._native.dispatch(
            scope.get("method", "GET"),
            scope.get("path", "/"),
            query_string,
            headers,
            b"".join(body_chunks),
        )

        encoded_headers = [
            (
                name.encode("utf-8") if isinstance(name, str) else name,
                value.encode("utf-8") if isinstance(value, str) else value,
            )
            for name, value in response_headers
        ]
        await send({"type": "http.response.start", "status": status, "headers": encoded_headers})
        await send({"type": "http.response.body", "body": response_body, "more_body": False})

    def openapi_schema(self) -> dict[str, Any] | None:
        raw = self._native.openapi_schema_json()
        if raw is None:
            return None
        return json.loads(raw)

    def provided_route_patterns(self) -> list[str]:
        return json.loads(self._native.provided_route_patterns_json())
