from __future__ import annotations

from fastapi import FastAPI

from axum_asgi_bridge import (
    DelegatePathsMiddleware,
    demo_asgi_app,
    install_openapi_merger,
    missing_delegated_routes,
)


def test_openapi_includes_delegated_routes() -> None:
    app = FastAPI(title="openapi-complete")
    rust_app = demo_asgi_app()

    @app.get("/python-only")
    async def python_only() -> dict[str, str]:
        return {"source": "fastapi"}

    app.add_middleware(
        DelegatePathsMiddleware,
        delegated_app=rust_app,
        should_delegate=lambda path: path == "/" or path.startswith("/echo"),
    )
    install_openapi_merger(app, delegated_app=rust_app, mount_prefix="")

    merged = app.openapi()

    missing = missing_delegated_routes(
        merged_schema=merged,
        delegated_routes=rust_app.provided_route_patterns(),
        mount_prefix="",
    )
    assert missing == []
    assert "/python-only" in merged.get("paths", {})
    assert "/echo" in merged.get("paths", {})
