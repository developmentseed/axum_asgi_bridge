from __future__ import annotations

import pytest
from axum_asgi_bridge import InvalidRequestError
from axum_asgi_bridge._native import demo_app as _demo_native


async def test_invalid_method_maps_to_invalid_request_error() -> None:
    native = _demo_native()
    with pytest.raises(InvalidRequestError):
        await native.dispatch("GET", "/", "", [("bad header", "value")], b"")
