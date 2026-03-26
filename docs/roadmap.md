# Roadmap

Nine areas identified for future development, ordered roughly from most impactful
to most specialized. Each includes motivation, design, and a concrete implementation
plan.

---

## 1. Streaming Response Support

### Motivation

`AxumAsgiBridge` currently buffers the complete response body before returning it
to the ASGI layer. This means:

- A 100 MB file download allocates 100 MB in the Rust heap before any bytes
  reach the client.
- Server-Sent Events (SSE) and chunked responses can't begin streaming until
  the Rust handler completes — which, for SSE, is never.
- Large paginated API responses (e.g., 10 000-item GeoJSON FeatureCollections)
  have artificially high time-to-first-byte.

### Design

ASGI streaming works by sending multiple `http.response.body` events with
`more_body: True` terminated by a final event with `more_body: False`:

```text
send({"type": "http.response.start", "status": 200, "headers": [...]})
send({"type": "http.response.body", "body": b"chunk1", "more_body": True})
send({"type": "http.response.body", "body": b"chunk2", "more_body": True})
send({"type": "http.response.body", "body": b"",     "more_body": False})
```

The Rust bridge must expose the response body as a stream that the Python wrapper
iterates, calling `send()` for each chunk.

**PyO3 async generator approach:**

```rust
// Return an async iterator of (Option<Vec<u8>>, bool)
// true = more_body, false = final chunk
fn dispatch_streaming<'py>(
    &self,
    py: Python<'py>,
    ...
) -> PyResult<Bound<'py, PyAny>> {
    // Convert Body stream into Python async generator
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Return response headers synchronously, body as async iter
    })
}
```

A more ergonomic approach is to accept the ASGI `send` callable and drive it
from Rust:

```rust
fn dispatch_to_send<'py>(
    &self,
    py: Python<'py>,
    send: Py<PyAny>,
    ...
) -> PyResult<Bound<'py, PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let response = self.router.clone().oneshot(request).await?;
        let status = response.status().as_u16();
        let headers = collect_headers(&response);

        // Send start event
        send.call1(py, (start_event,))?;

        // Stream body chunks
        let mut stream = response.into_body().into_data_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let more = !stream.is_terminated();
            send.call1(py, (body_event(chunk, more),))?;
        }
        Ok(())
    })
}
```

### Implementation Plan

1. Add `dispatch_streaming` method to `AxumAsgiBridge` that accepts the Tower
   response body as a `Stream<Item = Bytes>`.
2. In the PyO3 binding, drive the ASGI `send` callable from within the Rust
   async block using `pyo3_async_runtimes`.
3. Update `AxumAsgiApp.__call__` to detect whether the native object supports
   `dispatch_streaming` and fall back to buffered dispatch if not.
4. Add a streaming handler to the demo router (SSE or chunked JSON).
5. Add Python tests using `httpx.AsyncClient` with streaming response support.

### Files to Change

- `src/bridge.rs` — add `dispatch_streaming`
- `src/lib.rs` — add `PyAxumAsgiBridge::dispatch_streaming`
- `python/axum_asgi_bridge/asgi.py` — update `__call__` for streaming
- `tests/dispatch_raw.rs` — add streaming test
- `tests/python/test_streaming.py` — new test file

---

## 2. WebSocket Support

### Motivation

WebSocket connections arrive as ASGI `websocket` scope events. Many real-time
applications use WebSockets for live data, notifications, and bidirectional
communication. A bridge that only handles `http` scope is limited to
request/response patterns.

### ASGI WebSocket Protocol

```text
receive → {"type": "websocket.connect"}
send    → {"type": "websocket.accept"}
receive → {"type": "websocket.receive", "text": "...", "bytes": None}
send    → {"type": "websocket.send",    "text": "...", "bytes": None}
receive → {"type": "websocket.disconnect", "code": 1000}
send    → {"type": "websocket.close", "code": 1000}
```

### Design

Axum's WebSocket support (`axum::extract::WebSocketUpgrade`) uses a callback
model: the upgrade is accepted and a handler closure receives a `WebSocket`
message stream. Bridging this to ASGI requires bidirectional channel plumbing.

The key insight is that an ASGI WebSocket connection maps to two Tokio channels:

```text
ASGI receive ──→ tokio::mpsc::Sender<Message> ──→ Axum WebSocket read
Axum WebSocket write ──→ tokio::mpsc::Sender<Message> ──→ ASGI send
```

**Python side:**

```python
async def __call__(self, scope, receive, send):
    if scope["type"] == "websocket":
        await self._native.dispatch_websocket(scope, receive, send)
        return
    # existing http handling ...
```

**Rust side:**

```rust
pub async fn dispatch_websocket(
    &self,
    path: String,
    headers: Vec<(String, String)>,
    receive: Py<PyAny>,   // ASGI receive callable
    send: Py<PyAny>,      // ASGI send callable
) -> Result<()> {
    // Build upgrade request
    // Spawn task bridging ASGI events ↔ WebSocket messages
}
```

### Implementation Plan

1. Research `axum::extract::WebSocketUpgrade` — it requires the raw HTTP upgrade
   request, which can be synthesized similarly to how HTTP requests are built today.
2. Create `WebSocketScope` struct (analogous to `AsgiHttpScope`).
3. Implement bidirectional channel bridge using two `tokio::spawn` tasks:
   - Task A: reads from ASGI `receive`, converts to `axum::extract::ws::Message`, sends to Axum
   - Task B: reads from Axum WebSocket, converts to ASGI send events
4. Add `dispatch_websocket` to `AxumAsgiBridge` and expose via PyO3.
5. Update middleware to pass `websocket` scopes through to the delegated app.
6. Add demo WebSocket echo handler to the demo router.
7. Add tests using ASGI WebSocket test client (httpx or pytest-anyio).

### Files to Change

- `src/bridge.rs` — add `WebSocketScope`, `dispatch_websocket`
- `src/lib.rs` — expose `dispatch_websocket` via PyO3
- `python/axum_asgi_bridge/asgi.py` — handle `websocket` scope
- `tests/python/test_websocket.py` — new test file

---

## 3. ASGI Lifespan Events

### Motivation

ASGI servers send `lifespan` scope events on startup and shutdown:

```text
receive → {"type": "lifespan.startup"}
send    → {"type": "lifespan.startup.complete"}
receive → {"type": "lifespan.shutdown"}
send    → {"type": "lifespan.shutdown.complete"}
```

Currently the bridge silently drops these. Applications that need Rust-side
resource initialization (database pools, background tasks, file handles) have
no hook to do so at the right time.

### Design

Expose optional `on_startup` and `on_shutdown` async Rust callbacks:

```rust
pub struct AxumAsgiBridge {
    router: Router,
    // ...
    on_startup: Option<Box<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>>,
    on_shutdown: Option<Box<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>>,
}

impl AxumAsgiBridge {
    pub fn with_startup<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    { ... }
}
```

**Python side:**

```python
async def __call__(self, scope, receive, send):
    if scope["type"] == "lifespan":
        await self._handle_lifespan(receive, send)
        return
    # ...

async def _handle_lifespan(self, receive, send):
    event = await receive()
    if event["type"] == "lifespan.startup":
        try:
            await self._native.on_startup()
            await send({"type": "lifespan.startup.complete"})
        except Exception as exc:
            await send({"type": "lifespan.startup.failed", "message": str(exc)})
    event = await receive()
    if event["type"] == "lifespan.shutdown":
        await self._native.on_shutdown()
        await send({"type": "lifespan.shutdown.complete"})
```

### Integration with FastAPI

FastAPI uses context manager lifespan:

```python
from contextlib import asynccontextmanager
from fastapi import FastAPI

@asynccontextmanager
async def lifespan(app):
    await rust_app._native.on_startup()
    yield
    await rust_app._native.on_shutdown()

app = FastAPI(lifespan=lifespan)
```

A helper function `install_lifespan(app, bridge_app)` should wrap this pattern.

### Implementation Plan

1. Add `on_startup` / `on_shutdown` `Option<Box<dyn Fn() -> ...>>` fields to `AxumAsgiBridge`.
2. Add `with_startup` / `with_shutdown` builder methods.
3. Expose `on_startup()` and `on_shutdown()` as async PyO3 methods.
4. Update `AxumAsgiApp.__call__` to handle `lifespan` scope.
5. Add `install_lifespan(fastapi_app, bridge_app)` helper to `integrations.py`.
6. Update `DelegatePathsMiddleware` to forward lifespan events to both apps.
7. Add tests proving startup/shutdown callbacks fire in order.

### Files to Change

- `src/bridge.rs` — lifespan callback fields + builder methods
- `src/lib.rs` — PyO3 `on_startup()` / `on_shutdown()`
- `python/axum_asgi_bridge/asgi.py` — lifespan scope handling
- `python/axum_asgi_bridge/integrations.py` — `install_lifespan`
- `python/axum_asgi_bridge/__init__.py` — export `install_lifespan`
- `tests/python/test_lifespan.py` — new test file

---

## 4. Tower Middleware Integration

### Motivation

[Tower](https://github.com/tower-rs/tower) is the middleware framework underlying
Axum. A rich ecosystem of off-the-shelf Tower layers exists:

| Layer | Use case |
|---|---|
| `tower_http::compression` | gzip/brotli/deflate response compression |
| `tower_http::cors` | CORS headers |
| `tower_http::trace` | OpenTelemetry-compatible tracing |
| `tower_http::timeout` | Per-request timeout |
| `tower_http::limit` | Request body size limiting |
| `tower_http::set_header` | Inject static response headers |

Currently users must wrap their `Router` with these layers manually before
passing it to `AxumAsgiBridge::new()`. The builder pattern should make common
cases ergonomic.

### Design

```rust
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

let bridge = AxumAsgiBridge::new(my_router())
    .with_layer(CompressionLayer::new())
    .with_layer(CorsLayer::permissive())
    .with_layer(TraceLayer::new_for_http());
```

This requires changing `AxumAsgiBridge` to hold a `BoxCloneService` instead of
a `Router` after layers are applied:

```rust
use tower::util::BoxCloneService;
use axum::http::{Request, Response};
use axum::body::Body;

pub struct AxumAsgiBridge {
    service: BoxCloneService<Request<Body>, Response<Body>, Box<dyn StdError + Send + Sync>>,
    // ...
}
```

The `Router` is converted to a `BoxCloneService` when the first layer is added
or when the bridge is first used.

### Builder ergonomics

```rust
pub fn with_layer<L>(self, layer: L) -> Self
where
    L: Layer<Router> + Clone + Send + Sync + 'static,
    L::Service: Service<Request<Body>> + Clone + Send + Sync + 'static,
    // ...
```

### Python bindings

Expose common configurations as Python-side helpers:

```python
def with_compression(bridge: PyAxumAsgiBridge) -> PyAxumAsgiBridge:
    """Add gzip/brotli response compression."""
    return bridge._native.with_compression()
```

### Implementation Plan

1. Add `tower-http` as an optional dependency (`features = ["compression", "cors", "trace", "timeout"]`).
2. Change internal storage from `Router` to an `Arc<dyn ...>` service after build.
3. Add `with_layer` builder method to `AxumAsgiBridge`.
4. Add convenience builders: `with_compression()`, `with_cors(origins)`, `with_timeout(duration)`.
5. Expose convenience builders as PyO3 methods on `PyAxumAsgiBridge`.
6. Add tests verifying compressed responses, CORS headers, timeouts.
7. Document as a new "Middleware" page in the docs.

### Files to Change

- `Cargo.toml` — add `tower-http` optional dependency
- `src/bridge.rs` — service boxing, `with_layer`, convenience builders
- `src/lib.rs` — PyO3 bindings for convenience builders
- `docs/middleware.md` — new documentation page

---

## 5. Automatic Route Pattern Extraction

### Motivation

Declaring route patterns manually in `with_route_patterns(...)` is error-prone:

- Forgetting to declare a route means it won't appear in `provided_route_patterns()`
- Typos create invisible inconsistencies between the actual router and the declared patterns
- Adding a route to the Axum router requires a second change in the bridge builder

Axum doesn't expose matched route patterns from a `Router` after build, but the
patterns are knowable at construction time.

### Design Options

**Option A: Macro-based registration**

```rust
axum_routes! {
    Router::new()
        .route("/items",     get(list_items))
        .route("/items/:id", get(get_item))
} // automatically registers ["/items", "/items/:id"]
```

Implemented as a proc-macro that intercepts `.route(path, ...)` calls and
accumulates the path strings.

**Option B: `RouteRegistry` wrapper type (recommended)**

```rust
pub struct RouteRegistry {
    router: Router,
    patterns: Vec<String>,
}

impl RouteRegistry {
    pub fn new() -> Self { ... }

    pub fn route(mut self, path: &str, method_router: MethodRouter) -> Self {
        self.patterns.push(path.to_owned());
        self.router = self.router.route(path, method_router);
        self
    }

    pub fn into_bridge(self) -> AxumAsgiBridge {
        AxumAsgiBridge::new(self.router)
            .with_route_patterns(self.patterns)
    }
}
```

Usage:

```rust
let bridge = RouteRegistry::new()
    .route("/items",     get(list_items))
    .route("/items/:id", get(get_item))
    .into_bridge()
    .with_openapi_schema(schema);
```

Option B is preferred: it has zero macro complexity, is visible to `cargo doc`,
and works with standard `use` imports.

**Option C: Router extension trait**

```rust
trait RouterExt {
    fn route_tracked(self, path: &str, method_router: MethodRouter) -> (Self, Vec<String>);
}
```

This is awkward because callers must pass pattern accumulation state through
the chain.

### Implementation Plan

1. Add `RouteRegistry` struct to `src/bridge.rs` (or new `src/registry.rs`).
2. Implement `route()`, `nest()`, `merge()` mirrors that forward to the inner
   `Router` and accumulate patterns.
3. Add `into_bridge()` that returns an `AxumAsgiBridge` with patterns pre-filled.
4. Export `RouteRegistry` from the crate public API.
5. Update the demo router in `src/lib.rs` to use `RouteRegistry`.
6. Add tests verifying `provided_route_patterns()` matches constructed routes.
7. Update docs to show `RouteRegistry` as the recommended pattern.

### Files to Change

- `src/bridge.rs` (or new `src/registry.rs`) — `RouteRegistry`
- `src/lib.rs` — update demo to use `RouteRegistry`
- `tests/dispatch_raw.rs` — add `RouteRegistry` test
- `docs/api.md` — document `RouteRegistry`

---

## 6. OpenAPI Auto-Generation (utoipa / aide)

### Motivation

Hand-writing `serde_json::json!({...})` OpenAPI schemas is:

- Tedious for all but trivial APIs
- Prone to drift from the actual handler signatures
- Not validated at compile time
- Missing request body schemas, response types, parameter descriptions

Two mature Rust crates derive OpenAPI from handler type signatures:

- **[utoipa](https://github.com/juhaku/utoipa)** — macro-based, works with Axum
- **[aide](https://github.com/tamasfe/aide)** — functional API, tight Axum integration

### Design (utoipa)

```rust
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

#[derive(ToSchema, serde::Serialize)]
struct ItemList {
    items: Vec<Item>,
}

#[utoipa::path(
    get,
    path = "/items",
    responses(
        (status = 200, description = "List items", body = ItemList)
    )
)]
async fn list_items() -> Json<ItemList> { ... }

#[derive(OpenApi)]
#[openapi(paths(list_items))]
struct ApiDoc;

// In bridge builder:
let (router, api) = OpenApiRouter::new()
    .routes(routes!(list_items))
    .split_for_parts();

let schema = api.to_value();  // serde_json::Value
let bridge = AxumAsgiBridge::new(router)
    .with_openapi_schema(schema);
```

### Integration helper

Add a builder method that accepts a utoipa `OpenApi` directly:

```rust
#[cfg(feature = "utoipa")]
pub fn with_utoipa_schema<A: utoipa::OpenApi>(self) -> Self {
    self.with_openapi_schema(serde_json::to_value(A::openapi()).unwrap())
}
```

### Implementation Plan

1. Add `utoipa` and `utoipa-axum` as optional dependencies behind a `"utoipa"` feature flag.
2. Add `with_utoipa_schema<A: utoipa::OpenApi>()` method to `AxumAsgiBridge`.
3. Update the demo router to use utoipa derivations.
4. Add a `features = ["utoipa"]` example in `examples/utoipa_fastapi.py`.
5. Consider an `aide` alternative behind an `"aide"` feature (mutually exclusive or additive).
6. Document in `docs/api.md` under a "OpenAPI generation" section.
7. Update CI to run tests with `--features utoipa`.

### Files to Change

- `Cargo.toml` — add `utoipa`, `utoipa-axum` optional dependencies
- `src/lib.rs` — update demo router, add feature-gated builder method
- `src/bridge.rs` — `with_utoipa_schema` method
- `examples/utoipa_fastapi.py` — new example
- `docs/api.md` — utoipa section

---

## 7. Typed Python Exception Classes

### Motivation

All error paths in the bridge currently raise `RuntimeError`:

```python
PyRuntimeError::new_err(e.to_string())
```

This makes programmatic error handling impossible — callers can't distinguish
between a routing failure, an invalid request, and a handler panic:

```python
try:
    await rust_app(scope, receive, send)
except RuntimeError as exc:
    # Is this a bad request? A 500? A bridge bug?
    log.error("something went wrong: %s", exc)
```

### Design

Define a hierarchy matching `BridgeError` variants:

```python
# python/axum_asgi_bridge/exceptions.py

class BridgeError(Exception):
    """Base class for all axum_asgi_bridge errors."""

class BridgeDispatchError(BridgeError):
    """Error dispatching the request through the Axum router."""

class BridgeConfigError(BridgeError):
    """Error in bridge configuration (bad schema JSON, etc.)."""

class InvalidRequestError(BridgeError):
    """Malformed request — invalid method, URI, or header."""
    
class ResponseBodyError(BridgeError):
    """Error reading the Axum response body."""
```

On the Rust side, register each exception and raise the right one:

```rust
// In #[pymodule]
m.add("BridgeError", py.get_type::<PyBridgeError>())?;
m.add("InvalidRequestError", py.get_type::<PyInvalidRequestError>())?;
// ...

// In dispatch binding:
BridgeError::InvalidMethod(_) | BridgeError::InvalidUri(_) | ... => {
    Err(PyInvalidRequestError::new_err(e.to_string()))
}
BridgeError::Service(_) => {
    Err(PyBridgeDispatchError::new_err(e.to_string()))
}
```

### Python usage after this change

```python
from axum_asgi_bridge.exceptions import InvalidRequestError, BridgeDispatchError

try:
    await rust_app(scope, receive, send)
except InvalidRequestError:
    # Return 400 to client
except BridgeDispatchError:
    # Log and return 500
```

### Implementation Plan

1. Create `python/axum_asgi_bridge/exceptions.py` with the exception hierarchy.
2. Add `create_exception!` calls in `src/lib.rs` to register PyO3 exception types.
3. Update all `PyRuntimeError::new_err` call sites to use the specific type.
4. Export exception classes from `__init__.py`.
5. Update `AxumAsgiApp.__call__` to catch and re-raise as appropriate.
6. Add tests asserting correct exception types are raised for invalid inputs.
7. Document exceptions in `docs/api.md`.

### Files to Change

- `python/axum_asgi_bridge/exceptions.py` — new file
- `python/axum_asgi_bridge/__init__.py` — export exceptions
- `src/lib.rs` — register exception types, update raise sites
- `tests/python/test_exceptions.py` — new test file
- `docs/api.md` — exceptions section

---

## 8. Metrics and Tracing

### Motivation

Production services need observability. Today, requests dispatched through the
bridge are invisible to both:

- **Rust-side tracing** — `tower_http::trace::TraceLayer` spans (see item 4)
- **Python-side metrics** — Prometheus counters, histograms in `prometheus_client`

Adding first-class observability support enables:

- Per-route latency histograms (p50, p95, p99)
- Error rate tracking
- OpenTelemetry trace propagation across the Python/Rust boundary
- Structured logs with request context

### Design

**Python-side (callback hooks):**

```python
class AxumAsgiApp:
    def __init__(self, native_app, *, on_request_done=None):
        self._native = native_app
        self._on_request_done = on_request_done

    async def __call__(self, scope, receive, send):
        start = time.perf_counter()
        ...
        status, headers, body = await self._native.dispatch(...)
        elapsed = time.perf_counter() - start
        if self._on_request_done:
            self._on_request_done(
                method=scope["method"],
                path=scope["path"],
                status=status,
                duration_s=elapsed,
            )
```

**Prometheus integration (optional dependency):**

```python
from axum_asgi_bridge.metrics import PrometheusMetricsHook

rust_app = AxumAsgiApp(
    _demo_native(),
    on_request_done=PrometheusMetricsHook(
        prefix="axum_asgi_bridge",
        buckets=[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0],
    ),
)
```

**Rust-side tracing (`tracing` crate):**

```rust
pub async fn dispatch_scope(&self, scope: AsgiHttpScope, body: Vec<u8>) -> Result<DispatchResult> {
    let span = tracing::info_span!(
        "axum_asgi_bridge.dispatch",
        http.method = %scope.method,
        http.path = %scope.path,
    );
    let _enter = span.enter();
    // ... dispatch ...
    tracing::info!(http.status = status, "request complete");
}
```

**OpenTelemetry trace context propagation:**

Extract `traceparent` / `tracestate` headers from the incoming request and
attach them to the Rust span, so Rust and Python appear in the same trace tree.
Requires `opentelemetry-otlp` or `tracing-opentelemetry`.

### Implementation Plan

1. Add `on_request_done` callback parameter to `AxumAsgiApp.__init__`.
2. Measure wall-clock time around `dispatch()` in `__call__`.
3. Create `python/axum_asgi_bridge/metrics.py` with `PrometheusMetricsHook` (optional dep on `prometheus_client`).
4. Add `tracing` and `tracing-subscriber` as optional Rust dependencies.
5. Instrument `dispatch_scope` with `tracing::info_span!`.
6. Add `with_tracing()` builder method that installs `tower_http::trace::TraceLayer`.
7. Document in a new `docs/observability.md` page.

### Files to Change

- `Cargo.toml` — add `tracing`, `tracing-subscriber` optional deps
- `src/bridge.rs` — add tracing spans
- `python/axum_asgi_bridge/asgi.py` — add `on_request_done` hook
- `python/axum_asgi_bridge/metrics.py` — new file (optional)
- `python/axum_asgi_bridge/__init__.py` — export metrics hook
- `pyproject.toml` — add `prometheus` optional extra
- `docs/observability.md` — new page

---

## 9. GIL Release for Synchronous Methods

### Motivation

`openapi_schema_json()` and `provided_route_patterns_json()` are synchronous
PyO3 methods that currently hold the GIL for their entire execution. In a
threaded ASGI server (e.g., Uvicorn with multiple workers sharing a process, or
a `ThreadPoolExecutor` calling Python), this means other threads are blocked for
the duration of JSON serialization.

These methods are typically called once at startup but could be called in
request handlers that build dynamic schemas.

### Design

For synchronous functions with no Python interaction inside, `py.allow_threads`
releases the GIL:

```rust
fn openapi_schema_json(&self, py: Python<'_>) -> PyResult<Option<String>> {
    let schema_ref = self.inner.openapi_schema.as_ref();
    py.allow_threads(|| {
        // This runs without the GIL — no Python objects touched
        schema_ref
            .map(|schema| {
                serde_json::to_string(schema)
                    .map_err(|e| PyRuntimeError::new_err(e.to_string()))
            })
            .transpose()
    })
}
```

The critical constraint is that the closure must not touch any `Py<T>` or
`Bound<'_, T>` objects — GIL re-entry would deadlock. Since `openapi_schema_json`
and `provided_route_patterns_json` only operate on `serde_json::Value` and
`BTreeSet<String>`, this is safe.

### Complexity and trade-offs

| Aspect | Analysis |
|---|---|
| **Benefit** | Other Python threads can run during JSON serialization |
| **When it matters** | Multi-threaded server, large schemas (>10 KB), schema generated on request |
| **When it doesn't** | Schema cached after first call (typical FastAPI usage), single-threaded server |
| **Risk** | Must ensure no `Py<T>` objects cross the `allow_threads` boundary |
| **Implementation effort** | Low — change 2 functions, add `py: Python<'_>` parameters |

For most deployments this optimization has negligible practical impact because
the schema is generated once and cached by `install_openapi_merger`. However,
it is a correctness improvement for libraries that build on this crate and call
schema methods frequently.

### Implementation Plan

1. Add explicit `py: Python<'_>` parameter to `openapi_schema_json` and
   `provided_route_patterns_json` in the PyO3 `#[pymethods]` block.
2. Wrap the JSON serialization inside `py.allow_threads(|| { ... })`.
3. Verify the closure body contains no `Py<T>` values (static analysis via `Send` bounds).
4. Add a comment explaining why `allow_threads` is safe here.
5. Update `AxumAsgiApp` Python wrapper — no changes needed (Python side doesn't
   call these on the hot path).
6. Add a test with `concurrent.futures.ThreadPoolExecutor` calling `openapi_schema_json`
   concurrently to confirm no deadlock.

### Files to Change

- `src/lib.rs` — update `openapi_schema_json` and `provided_route_patterns_json`
- `tests/python/test_threadsafety.py` — new test file

---

## Priority and Dependencies

```text
                  ┌───────────────┐
                  │  1. Streaming │  (independent, high value)
                  └───────────────┘
                  ┌───────────────┐
                  │  2. WebSocket │  (independent, niche)
                  └───────────────┘
       ┌─────────────────────────────────────┐
       │  3. Lifespan  →  depends on 4 (optional Tower lifecycle)
       └─────────────────────────────────────┘
       ┌─────────────────────────────────────┐
       │  4. Middleware  →  5. Route extract (RouteRegistry can wrap after)
       └─────────────────────────────────────┘
                  ┌───────────────┐
                  │  6. utoipa    │  (independent, high value for adopters)
                  └───────────────┘
                  ┌───────────────┐
                  │  7. Exceptions│  (independent, low effort)
                  └───────────────┘
       ┌─────────────────────────────────────┐
       │  8. Metrics  →  benefits from 4 (TraceLayer)
       └─────────────────────────────────────┘
                  ┌───────────────┐
                  │  9. GIL       │  (independent, low effort)
                  └───────────────┘
```

**Recommended sequence:**

1. Items 7 and 9 — low effort, high correctness value
2. Item 5 (RouteRegistry) — eliminates manual error-prone setup
3. Item 6 (utoipa) — enables real-world API adoption
4. Item 4 (middleware) — unlocks compression, CORS, tracing
5. Item 3 (lifespan) — enables production startup/shutdown hooks
6. Item 1 (streaming) — needed for SSE and large responses
7. Item 8 (metrics) — builds on 4 and 1
8. Item 2 (WebSocket) — significant complexity, specialized use case
