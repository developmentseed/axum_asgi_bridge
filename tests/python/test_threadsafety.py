from __future__ import annotations

from concurrent.futures import ThreadPoolExecutor

from axum_asgi_bridge._native import demo_app as _demo_native


def test_openapi_and_routes_methods_are_thread_safe() -> None:
    native = _demo_native()

    def call_metadata() -> tuple[str | None, str]:
        return native.openapi_schema_json(), native.provided_route_patterns_json()

    with ThreadPoolExecutor(max_workers=8) as pool:
        results = list(pool.map(lambda _idx: call_metadata(), range(64)))

    assert all(result[0] is not None for result in results)
    assert all(result[1].startswith("[") for result in results)
