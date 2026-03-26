from __future__ import annotations

from fastapi import FastAPI

from axum_asgi_bridge import demo_asgi_app


app = FastAPI(title="axum_asgi_bridge example")
app.mount("/rust", demo_asgi_app())
