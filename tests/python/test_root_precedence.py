from __future__ import annotations

from fastapi import FastAPI
from httpx import ASGITransport, AsyncClient

from axum_asgi_bridge import DelegatePathsMiddleware, demo_asgi_app


def make_app() -> FastAPI:
    app = FastAPI()
    rust_app = demo_asgi_app()

    @app.get("/")
    async def fastapi_root() -> dict[str, str]:
        return {"source": "fastapi"}

    @app.get("/python-only")
    async def python_only() -> dict[str, str]:
        return {"source": "fastapi", "route": "python-only"}

    app.add_middleware(
        DelegatePathsMiddleware,
        delegated_app=rust_app,
        should_delegate=lambda path: path == "/" or path.startswith("/echo"),
    )
    return app


async def test_root_path_is_intercepted_by_rust_delegate() -> None:
    app = make_app()
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/")

    assert response.status_code == 200
    payload = response.json()
    assert payload.get("service") == "axum_asgi_bridge"
    assert payload.get("source") != "fastapi"


async def test_non_delegated_path_still_uses_fastapi() -> None:
    app = make_app()
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/python-only")

    assert response.status_code == 200
    assert response.json() == {"source": "fastapi", "route": "python-only"}
