***EXPERIMENTAL***

Right now this is a test of this approach and should not be considered production ready. Everything here is subject to complete rewrite, abandonment, or API changes.

# axum_asgi_bridge

Embed high-performance [Axum](https://github.com/tokio-rs/axum) services directly into Python ASGI hosts such as [FastAPI](https://fastapi.tiangolo.com/) and [Starlette](https://www.starlette.io/).

## Why?

When parts of your API benefit from Rust's speed — compute-heavy endpoints, high-throughput data pipelines, or latency-sensitive routes — you shouldn't have to choose between Rust and Python. `axum_asgi_bridge` lets you write those routes in Axum and compose them seamlessly into an existing Python application, sharing the same process and the same OpenAPI docs.

In practice, this gives you a hybrid service model:

- Keep existing FastAPI or Starlette routes, dependency injection, auth, and ecosystem tooling.
- Move selected hot paths to Rust without introducing a separate microservice hop.
- Deploy as a single ASGI app while still using Axum and Tower for performance-critical handlers.

## When To Use This

Use this bridge when all of the following are true:

- You already have a Python ASGI application and want to keep it.
- Only some endpoints are bottlenecks, not the whole system.
- You want Rust performance without network overhead between Python and Rust services.
- You can tolerate a mixed-language codebase in exchange for throughput or latency wins.

Common good fits:

- STAC or geospatial APIs with expensive filtering/serialization.
- CPU-heavy transformation endpoints.
- Streaming or high-throughput endpoints where Python overhead is measurable.

Less ideal fits:

- Very small APIs where operational simplicity matters more than performance.
- Teams not ready to own Rust + Python build and release pipelines.
- Workloads dominated by remote database latency, where bridge overhead is not the bottleneck.

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

## Performance Profile

### Where It Helps

- No per-request JSON encode/decode between Python and Rust for normal dispatch.
- Fast Rust execution for routing, middleware, and handler logic.
- No extra network hop compared with a separate Rust microservice.
- `dispatch_to_send` path streams response frames to ASGI `send` with backpressure-aware awaits.

### Where It Still Costs

- Python↔Rust boundary crossing still has conversion cost for headers/body and call scheduling.
- Request bodies are copied into Rust-owned buffers.
- You pay complexity cost: PyO3/maturin build tooling, ABI-compatible wheels, and dual-language debugging.
- Some Axum-native capabilities (notably extractor upgrade websocket path) are constrained by embedding mode.

### Rule Of Thumb

- If your endpoint spends most time in Python compute or JSON handling, this bridge can deliver meaningful gains.
- If your endpoint mostly waits on external IO (database/network), expect smaller gains unless Rust-side work is substantial.

Measure with your real payloads and concurrency levels before committing to migration breadth.

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

## Production Readiness Checklist

- Build in release mode (`uv run maturin develop --release` or wheel build).
- Run `scripts/check.sh` in CI for every change.
- Keep delegated route list synchronized with router changes (or use `RouteRegistry`).
- Enable timeout/compression/tracing layers when needed (`middleware`/`observability` features).
- Add request metrics hooks (`on_request_done`, `PrometheusMetricsHook`) before launch.
- Validate behavior for large responses and websocket workloads in staging.

## Current Limits

- Axum extractor-native websocket upgrades are not available through `Router::oneshot` in this embedding path.
- `dispatch_to_send` is the preferred path for backpressure-driven HTTP streaming.
- `dispatch_streaming` remains available for compatibility and returns chunk vectors.

## Documentation

Build and serve locally:

```bash
uv run mkdocs serve
```

Or see the [docs/](docs/index.md) directory.

## License

Dual-licensed under [MIT](https://opensource.org/licenses/MIT) or [Apache-2.0](https://www.apache.org/licenses/LICENSE-2.0), at your option.
