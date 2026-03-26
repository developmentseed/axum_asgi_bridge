# axum_asgi_bridge

Embed high-performance [Axum](https://github.com/tokio-rs/axum) services directly into Python ASGI hosts such as [FastAPI](https://fastapi.tiangolo.com/) and [Starlette](https://www.starlette.io/).

## Why?

When parts of your API benefit from Rust's speed — compute-heavy endpoints, high-throughput data pipelines, or latency-sensitive routes — you shouldn't have to choose between Rust and Python. `axum_asgi_bridge` lets you write those routes in Axum and compose them seamlessly into an existing Python application, sharing the same process and the same OpenAPI docs.

## How it Works

```text
ASGI host (FastAPI / Starlette / Uvicorn)
  │
  ├─ Python routes ──→ handled normally
  │
  └─ Delegated routes ──→ AxumAsgiApp (Python ASGI wrapper)
                              │
                              └─ PyO3 native call (zero-JSON dispatch)
                                    │
                                    └─ AxumAsgiBridge::dispatch
                                          │
                                          └─ Axum Router (Tower Service)
```

Request data crosses the Python → Rust boundary via PyO3's native type conversions — **no JSON serialization on the hot path**. Response headers and body bytes flow back through PyO3 the same way.

## Features

- **Zero-JSON dispatch** — method, path, query string, headers, and body cross the boundary as native types, not JSON strings
- **ASGI-compliant** — drop-in `__call__` wrapper works with any ASGI server (Uvicorn, Hypercorn, Daphne)
- **FastAPI integration** — `DelegatePathsMiddleware` for route precedence, `install_openapi_merger` for unified `/docs`
- **OpenAPI completeness** — merge Rust-side OpenAPI schemas into FastAPI docs and assert nothing is missing
- **Full async** — Rust futures are exposed as Python awaitables via `pyo3-async-runtimes`
- **Optimized release builds** — fat LTO, single codegen unit, symbol stripping

## Install

### Development

```bash
uv sync --extra dev
uv run maturin develop --release
```

### From wheel

```bash
pip install axum_asgi_bridge
```

## Quick Start

### Standalone ASGI app

```python
from axum_asgi_bridge import demo_asgi_app

app = demo_asgi_app()
# Run with: uvicorn my_module:app
```

### Mount under FastAPI

```python
from fastapi import FastAPI
from axum_asgi_bridge import demo_asgi_app

app = FastAPI()
app.mount("/rust", demo_asgi_app())
```

### Rust routes take precedence over Python routes

```python
from fastapi import FastAPI
from axum_asgi_bridge import DelegatePathsMiddleware, demo_asgi_app

rust_app = demo_asgi_app()
app = FastAPI()

@app.get("/")
async def fastapi_root():
    return {"source": "fastapi"}

app.add_middleware(
    DelegatePathsMiddleware,
    delegated_app=rust_app,
    should_delegate=lambda path: path == "/",
)
# GET / is now handled by Rust
```

### Unified OpenAPI docs

```python
from fastapi import FastAPI
from axum_asgi_bridge import (
    demo_asgi_app,
    install_openapi_merger,
    missing_delegated_routes,
)

rust_app = demo_asgi_app()
app = FastAPI()
install_openapi_merger(app, delegated_app=rust_app, mount_prefix="")

# Verify all Rust routes appear in /docs
schema = app.openapi()
assert missing_delegated_routes(schema, rust_app.provided_route_patterns()) == []
```

## Run Tests

```bash
cargo test                   # Rust unit tests
uv run pytest -v             # Python integration tests
scripts/check.sh             # Full CI check (lint + test + docs)
```

## Documentation

Build and serve locally:

```bash
uv run mkdocs serve
```

Or see the [docs/](docs/index.md) directory.

## License

Dual-licensed under [MIT](https://opensource.org/licenses/MIT) or [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0), at your option.
