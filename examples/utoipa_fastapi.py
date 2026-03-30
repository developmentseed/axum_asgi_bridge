from __future__ import annotations

"""Placeholder example entrypoint for utoipa-enabled builds.

This project exposes `with_utoipa_schema` on the Rust bridge when compiled with
`--features utoipa`. The full schema derivation flow is implemented in Rust.
Use this file as the Python composition pattern once your native bridge exposes
an utoipa-derived schema.
"""

from axum_asgi_bridge import demo_asgi_app, install_openapi_merger
from fastapi import FastAPI

rust_app = demo_asgi_app()
app = FastAPI(title="axum_asgi_bridge utoipa example")
install_openapi_merger(app, delegated_app=rust_app)
