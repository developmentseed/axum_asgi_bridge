from __future__ import annotations

from fastapi import FastAPI

from axum_asgi_bridge import DelegatePathsMiddleware, demo_asgi_app


rust_app = demo_asgi_app()
app = FastAPI(title="axum_asgi_bridge root precedence example")


@app.get("/")
async def fastapi_root() -> dict[str, str]:
    return {"source": "fastapi"}


@app.get("/python-only")
async def python_only() -> dict[str, str]:
    return {"source": "fastapi", "route": "python-only"}


# Delegate '/' and '/echo*' to Rust first, so those paths take precedence over
# FastAPI routes with the same path.
app.add_middleware(
    DelegatePathsMiddleware,
    delegated_app=rust_app,
    should_delegate=lambda path: path == "/" or path.startswith("/echo"),
)
