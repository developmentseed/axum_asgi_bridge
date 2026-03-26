# axum_asgi_bridge

Embed high-performance Axum services directly into Python ASGI hosts.

## Overview

`axum_asgi_bridge` bridges the gap between Rust's Axum web framework and Python's ASGI ecosystem. It lets you write latency-sensitive or compute-heavy API routes in Rust and compose them into a FastAPI or Starlette application — same process, same OpenAPI docs, zero inter-process communication.

## Architecture

```text
┌──────────────────────────────────────────┐
│  ASGI Server (Uvicorn / Hypercorn)       │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  FastAPI / Starlette host app      │  │
│  │                                    │  │
│  │   Python routes ──→ normal         │  │
│  │                                    │  │
│  │   DelegatePathsMiddleware          │  │
│  │     │ should_delegate(path)?       │  │
│  │     └─→ AxumAsgiApp.__call__       │  │
│  │           │                        │  │
│  │           │ PyO3 native call        │  │
│  │           │ (method, path, qs,      │  │
│  │           │  headers, body)         │  │
│  │           ▼                        │  │
│  │  ┌─────────────────────────┐       │  │
│  │  │  Rust native extension  │       │  │
│  │  │                         │       │  │
│  │  │  AxumAsgiBridge         │       │  │
│  │  │    .dispatch(...)       │       │  │
│  │  │       │                 │       │  │
│  │  │       ▼                 │       │  │
│  │  │  Axum Router            │       │  │
│  │  │  (Tower Service)        │       │  │
│  │  │       │                 │       │  │
│  │  │       ▼                 │       │  │
│  │  │  (status, headers,      │       │  │
│  │  │   body_bytes)           │       │  │
│  │  └─────────────────────────┘       │  │
│  │           │                        │  │
│  │           ▼                        │  │
│  │   ASGI response.start + body       │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

### Data flow for a single request

1. The ASGI server receives an HTTP request and calls the host app.
2. `DelegatePathsMiddleware` checks whether the path should be handled by Rust.
3. If delegated, `AxumAsgiApp.__call__` collects the request body, decodes ASGI headers, and calls the native `dispatch()` method — **no JSON serialization**.
4. Inside Rust, `AxumAsgiBridge::dispatch` builds an `http::Request`, feeds it to the `axum::Router` via Tower's `oneshot()`, and collects the response.
5. Status code, response headers, and body bytes are returned to Python as native types via PyO3.
6. The ASGI response events are sent back to the server.

### Design decisions

| Decision | Rationale |
|---|---|
| **Zero-JSON dispatch** | JSON serialization/deserialization on every request is wasteful. PyO3 converts Python `str`, `list[tuple]`, and `bytes` to Rust types at near-zero cost. |
| **`Router.clone().oneshot()`** | Axum's `Router` is `Arc`-backed, so cloning is O(1). `oneshot()` is the canonical way to dispatch a single request through a Tower Service. |
| **`http-body-util` collect** | Body is collected via `BodyExt::collect().to_bytes()` which avoids manual chunk iteration and achieves zero-copy when the response is a single frame. |
| **Delegation middleware** | Mounting an ASGI app under a prefix changes its `root_path`. Middleware-based delegation keeps paths identical, allowing Rust to own `/` without path rewriting. |
| **Explicit route patterns** | Axum doesn't expose matched route patterns from a `Router`. Explicitly providing them enables OpenAPI completeness validation. |

## When to use this

- **Performance-critical routes** — JSON validation, spatial queries, cryptographic operations, large data transformations
- **Gradual migration** — Move hot endpoints to Rust one at a time without rewriting the entire API
- **Shared process** — Avoid the operational complexity of sidecar services or gRPC bridges
- **Unified documentation** — All routes appear in a single OpenAPI schema and Swagger UI
