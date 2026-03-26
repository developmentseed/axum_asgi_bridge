from __future__ import annotations

from httpx import ASGITransport, AsyncClient

from axum_asgi_bridge import demo_asgi_app


async def test_demo_app_root() -> None:
    app = demo_asgi_app()
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.get("/")
    assert response.status_code == 200
    assert response.json().get("ok") is True


async def test_demo_app_echo() -> None:
    app = demo_asgi_app()
    async with AsyncClient(transport=ASGITransport(app=app), base_url="http://test") as client:
        response = await client.post("/echo", content="hello")
    assert response.status_code == 200
    assert response.json() == {"echo": "hello"}
