# Quickstart

## Prerequisites

- **Rust** 1.85+ (for the 2024 edition)
- **Python** 3.10+
- **uv** (recommended) or pip
- **maturin** 1.8+ (installed automatically via `[dev]` extras)

## Development Setup

```bash
# Clone the repository
git clone https://github.com/stac-utils/axum_asgi_bridge.git
cd axum_asgi_bridge

# Install Python dependencies
uv sync --extra dev

# Build the native extension in release mode
uv run maturin develop --release
```

!!! tip
    Always use `--release` with `maturin develop`. The debug build is significantly
    slower due to missing optimizations, LTO, and debug assertions in Rust.

## Verify the installation

```bash
# Rust tests
cargo test

# Python tests
uv run pytest -v

# Full CI check (lint + test + docs build)
scripts/check.sh
```

## Integration Patterns

### 1. Standalone ASGI app

The simplest case — use the bridge app directly with any ASGI server:

```python
from axum_asgi_bridge import demo_asgi_app

app = demo_asgi_app()
```

```bash
uv run uvicorn my_module:app
```

### 2. Mount under a FastAPI prefix

Mount the Rust app at a sub-path. All Rust routes are served under `/rust/`:

```python
from fastapi import FastAPI
from axum_asgi_bridge import demo_asgi_app

app = FastAPI()
app.mount("/rust", demo_asgi_app())
```

### 3. Delegate specific paths to Rust

Use `DelegatePathsMiddleware` when Rust should take precedence over Python routes
**at the same path**. This is the recommended pattern for gradual migration:

```python
from fastapi import FastAPI
from axum_asgi_bridge import DelegatePathsMiddleware, demo_asgi_app

app = FastAPI()
rust_app = demo_asgi_app()

@app.get("/")
async def fastapi_root():
    return {"source": "fastapi"}

app.add_middleware(
    DelegatePathsMiddleware,
    delegated_app=rust_app,
    should_delegate=lambda path: path == "/" or path.startswith("/echo"),
)
```

Requests to `/` and `/echo*` are served by Rust. Requests to any other path
fall through to FastAPI.

### 4. Unified OpenAPI documentation

When using delegation, Rust routes won't appear in FastAPI's auto-generated docs
by default. Use `install_openapi_merger` to merge them:

```python
from fastapi import FastAPI
from axum_asgi_bridge import (
    demo_asgi_app,
    install_openapi_merger,
    missing_delegated_routes,
)

app = FastAPI()
rust_app = demo_asgi_app()

install_openapi_merger(app, delegated_app=rust_app, mount_prefix="")

# Assert completeness in tests
schema = app.openapi()
missing = missing_delegated_routes(schema, rust_app.provided_route_patterns())
assert missing == []
```

## Building a Custom Bridge

To use `axum_asgi_bridge` with your own Axum router instead of the demo:

```rust
use axum::{Router, Json, routing::get};
use axum_asgi_bridge::AxumAsgiBridge;
use serde_json::json;

fn my_router() -> Router {
    async fn items() -> Json<serde_json::Value> {
        Json(json!({"items": []}))
    }
    Router::new().route("/items", get(items))
}

// Create the bridge
let bridge = AxumAsgiBridge::new(my_router())
    .with_route_patterns(["/items".to_string()])
    .with_openapi_schema(json!({
        "openapi": "3.1.0",
        "info": {"title": "My API", "version": "1.0.0"},
        "paths": {
            "/items": {
                "get": {"responses": {"200": {"description": "List items"}}}
            }
        }
    }));
```

Wrap it in a `PyAxumAsgiBridge` (via `#[pyclass]`) and expose it as a `#[pyfunction]`
that returns the bridge. See [src/lib.rs](https://github.com/stac-utils/axum_asgi_bridge/blob/main/src/lib.rs)
for the pattern used by `demo_app`.

## Running Examples

```bash
# Basic FastAPI mount
uv run uvicorn examples.basic_fastapi:app --reload

# Root precedence with delegation
uv run uvicorn examples.root_precedence_fastapi:app --reload

# OpenAPI completeness check (runs and exits)
uv run python examples/openapi_complete_fastapi.py
```

## Troubleshooting

**`ModuleNotFoundError: No module named 'axum_asgi_bridge._native'`**
: Run `uv run maturin develop --release` to build and install the native extension.

**Slow request handling**
: Ensure you built with `--release`. Debug builds skip LTO and run ~10x slower.

**`RuntimeError: no running event loop`**
: The bridge requires an async context. Make sure you're running under an ASGI
  server (Uvicorn) or within `asyncio.run()`.

**Rust compilation errors after updating dependencies**
: Run `cargo update` then `uv run maturin develop --release` to rebuild.
