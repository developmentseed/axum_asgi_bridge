from __future__ import annotations

from fastapi import FastAPI

from axum_asgi_bridge import (
    DelegatePathsMiddleware,
    demo_asgi_app,
    install_openapi_merger,
    missing_delegated_routes,
)


rust_app = demo_asgi_app()
app = FastAPI(title="axum_asgi_bridge OpenAPI completeness example")


@app.get("/python-only")
async def python_only() -> dict[str, str]:
    return {"source": "fastapi", "route": "python-only"}


app.add_middleware(
    DelegatePathsMiddleware,
    delegated_app=rust_app,
    should_delegate=lambda path: path == "/" or path.startswith("/echo"),
)

# Ensure delegated Rust routes are included in FastAPI docs.
install_openapi_merger(app, delegated_app=rust_app, mount_prefix="")


def assert_openapi_complete() -> None:
    schema = app.openapi()
    missing = missing_delegated_routes(
        merged_schema=schema,
        delegated_routes=rust_app.provided_route_patterns(),
        mount_prefix="",
    )
    if missing:
        raise RuntimeError(f"OpenAPI missing delegated routes: {missing}")


if __name__ == "__main__":
    # Run this file to fail fast if delegated paths are absent in docs.
    assert_openapi_complete()
    print("OpenAPI completeness check passed")
