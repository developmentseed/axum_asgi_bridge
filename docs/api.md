# API Reference

## Python Package

### `AxumAsgiApp`

```python
class AxumAsgiApp:
    def __init__(
        self,
        native_app: Any,
        *,
        on_request_done: Callable[..., None] | None = None,
        stream_chunk_size: int = 0,
    ) -> None: ...
```

ASGI application wrapper around the native Rust bridge. Implements the ASGI
`__call__` protocol so it can be used with any ASGI server.

#### `__call__(scope, receive, send)`

Handles an ASGI request. Supports `http`, `lifespan`, and `websocket` scopes.
For `http`, collects the request body from `receive`, dispatches through the
native Rust bridge using zero-JSON structured arguments, and sends the response
via `send`.

**Parameters:**

| Name | Type | Description |
|---|---|---|
| `scope` | `dict[str, Any]` | ASGI connection scope |
| `receive` | `Callable` | ASGI receive channel |
| `send` | `Callable` | ASGI send channel |

#### `openapi_schema() -> dict | None`

Returns the OpenAPI schema provided by the Rust bridge, or `None` if no schema
was configured. Called once at startup, not on every request.

#### `provided_route_patterns() -> list[str]`

Returns the list of route patterns that the Rust bridge handles (e.g., `["/", "/echo"]`).
Used by `missing_delegated_routes` to verify OpenAPI completeness.

#### ASGI Scope Support

`AxumAsgiApp` handles:

- `http`
- `lifespan`
- `websocket` (only when native `dispatch_websocket` exists)

For `http`, the adapter prefers native `dispatch_to_send(...)` when available,
which forwards response frames to ASGI `send` with per-frame await semantics.

### `PrometheusMetricsHook`

```python
class PrometheusMetricsHook:
    def __init__(self, prefix: str = "axum_asgi_bridge", buckets: tuple[float, ...] | None = None) -> None: ...
```

Request metrics callback helper for `on_request_done`.

---

### `install_lifespan(app, delegated_app)`

Installs a FastAPI lifespan context wrapper that forwards startup/shutdown to
delegated bridge hooks when available.

---

### `demo_asgi_app() -> AxumAsgiApp`

```python
def demo_asgi_app() -> AxumAsgiApp: ...
```

Creates a demonstration ASGI app with two endpoints:

| Method | Path | Response |
|---|---|---|
| `GET` | `/` | `{"ok": true, "service": "axum_asgi_bridge"}` |
| `POST` | `/echo` | `{"echo": "<request body>"}` |

**Example:**

```python
from axum_asgi_bridge import demo_asgi_app
app = demo_asgi_app()
```

---

### `DelegatePathsMiddleware`

```python
class DelegatePathsMiddleware:
    def __init__(
        self,
        app: ASGIApp,
        delegated_app: ASGIApp,
        should_delegate: Callable[[str], bool] | None = None,
    ) -> None: ...
```

ASGI middleware that intercepts requests **before** the host app's router. When
`should_delegate(path)` returns `True`, the request is forwarded to `delegated_app`
instead of `app`.

**Parameters:**

| Name | Type | Description |
|---|---|---|
| `app` | `ASGIApp` | The host ASGI application (e.g., FastAPI) |
| `delegated_app` | `ASGIApp` | The bridge app to delegate to |
| `should_delegate` | `Callable[[str], bool]` | Predicate; returns `True` for paths Rust should handle. Defaults to `lambda _: False`. |

**Example:**

```python
from fastapi import FastAPI
from axum_asgi_bridge import DelegatePathsMiddleware, demo_asgi_app

app = FastAPI()
rust_app = demo_asgi_app()

app.add_middleware(
    DelegatePathsMiddleware,
    delegated_app=rust_app,
    should_delegate=lambda path: path in {"/", "/echo"},
)
```

!!! note
    `lifespan` scopes are forwarded to both delegated and host apps.
    `http` and `websocket` scopes are delegated when `should_delegate(path)` is `True`.

---

### `install_openapi_merger(app, delegated_app, mount_prefix="")`

```python
def install_openapi_merger(
    app: FastAPI,
    delegated_app: AxumAsgiApp,
    mount_prefix: str = "",
) -> None: ...
```

Monkey-patches `app.openapi()` to include the delegated app's OpenAPI schema.
The merged schema is cached after first generation, matching FastAPI's default behavior.

**Parameters:**

| Name | Type | Description |
|---|---|---|
| `app` | `FastAPI` | The host FastAPI application |
| `delegated_app` | `AxumAsgiApp` | Bridge app with `.openapi_schema()` |
| `mount_prefix` | `str` | Prefix prepended to delegated paths (e.g., `"/api/v1"`) |

---

### `missing_delegated_routes(merged_schema, delegated_routes, mount_prefix="")`

```python
def missing_delegated_routes(
    merged_schema: dict[str, Any],
    delegated_routes: list[str],
    mount_prefix: str = "",
) -> list[str]: ...
```

Returns route patterns from `delegated_routes` that are **not** present in the
merged OpenAPI schema's `paths`. Use this in tests to assert completeness.

**Returns:** List of missing route paths, e.g., `["/missing-route"]`. Empty list means all routes are documented.

**Example:**

```python
schema = app.openapi()
missing = missing_delegated_routes(schema, rust_app.provided_route_patterns())
assert missing == [], f"Undocumented routes: {missing}"
```

---

### `merge_openapi_with_delegate(base_schema, delegated_schema, mount_prefix="")`

```python
def merge_openapi_with_delegate(
    base_schema: dict[str, Any],
    delegated_schema: dict[str, Any] | None,
    mount_prefix: str = "",
) -> dict[str, Any]: ...
```

Low-level helper that deep-merges `delegated_schema` paths and components into
`base_schema`. Returns a new dict; `base_schema` is not modified.

Delegated paths are prepended with `mount_prefix`. Delegated components are
merged section-by-section (`schemas`, `securitySchemes`, etc.), with delegated
values taking precedence on key conflicts.

---

### `version() -> str`

Returns the version of the native Rust crate (e.g., `"0.1.0"`).

---

## Rust Crate

### `AxumAsgiBridge`

```rust
pub struct AxumAsgiBridge { /* ... */ }
```

The core bridge type. Wraps an `axum::Router` and provides methods to
dispatch HTTP requests and expose metadata.

#### `new(router: Router) -> Self`

Create a new bridge from an Axum router.

#### `with_openapi_schema(self, schema: serde_json::Value) -> Self`

Attach an OpenAPI schema (builder pattern).

#### `with_route_patterns(self, patterns: impl IntoIterator<Item = String>) -> Self`

Declare the route patterns this bridge handles (builder pattern).

#### `with_compression(self) -> Self` *(feature: `middleware`)*

Apply response compression middleware.

#### `with_cors_permissive(self) -> Self` *(feature: `middleware`)*

Apply permissive CORS middleware.

#### `with_timeout(self, duration: Duration) -> Self` *(feature: `middleware`)*

Apply per-request timeout middleware.

#### `with_trace_http(self) -> Self` *(feature: `middleware`)*

Apply Tower HTTP tracing layer.

#### `with_utoipa_schema<A: utoipa::OpenApi>(self) -> Self` *(feature: `utoipa`)*

Populate OpenAPI schema from a utoipa document type.

#### `async dispatch(&self, method, path, query_string, headers, body) -> Result<DispatchResult>`

Dispatch a request from structured arguments. This is the **fastest path** — no
JSON parsing. Used by the Python wrapper on the hot path.

| Parameter | Type | Description |
|---|---|---|
| `method` | `String` | HTTP method (`"GET"`, `"POST"`, etc.) |
| `path` | `String` | Request path (`"/items"`) |
| `query_string` | `String` | Query string without `?` (empty string = no query) |
| `headers` | `Vec<(String, String)>` | Request headers as name/value pairs |
| `body` | `Vec<u8>` | Request body bytes |

#### `async dispatch_response(&self, method, path, query_string, headers, body) -> Result<Response>`

Dispatch and return the raw `http::Response` so callers can stream body frames
without collecting the entire body.

#### `async dispatch_streaming(&self, method, path, query_string, headers, body) -> Result<DispatchStreamingResult>`

Dispatch and return body chunks as vectors for compatibility-oriented callers.

#### `dispatch_to_send(method, path, query_string, headers, body, send)` (Python native binding)

Dispatch and push `http.response.start` / `http.response.body` events directly
to ASGI `send`, awaiting each call before reading the next body frame.

#### `openapi_schema_json(&self) -> Result<Option<String>>`

Returns the attached OpenAPI schema as a JSON string, or `None`.

#### `provided_route_patterns_json(&self) -> Result<String>`

Returns the declared route patterns as a JSON array string.

### `DispatchResult`

```rust
pub struct DispatchResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}
```

Result of dispatching a request through the bridge.

### `AsgiHttpScope`

```rust
pub struct AsgiHttpScope {
    pub method: String,
    pub path: String,
    pub query_string: Option<String>,
    pub headers: Vec<(String, String)>,
}
```

In-memory representation of an ASGI HTTP scope.

### `RouteRegistry`

```rust
pub struct RouteRegistry { /* ... */ }
```

Builder wrapper that tracks registered routes and converts directly to
`AxumAsgiBridge` with route patterns populated.

Key methods:

- `RouteRegistry::new()`
- `route(path, method_router)`
- `nest(prefix, nested_registry)`
- `merge(other_registry)`
- `into_bridge()`

### `BridgeError`

```rust
pub enum BridgeError {
    InvalidMethod(String),
    InvalidUri(String),
    InvalidHeaderName(String),
    InvalidHeaderValue { name: String, message: String },
    Service(String),
    ResponseBody(String),
    JsonEncode { context: &'static str, message: String },
}
```

Error type covering all failure modes during dispatch.

## Python Exceptions

The native module exports typed exceptions:

- `BridgeError`
- `BridgeDispatchError`
- `BridgeConfigError`
- `InvalidRequestError`
- `ResponseBodyError`

These are re-exported by the package root for direct import.
