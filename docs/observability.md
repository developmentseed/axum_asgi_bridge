# Observability

## Python-side Request Hooks

`AxumAsgiApp` supports an `on_request_done` callback:

```python
from axum_asgi_bridge import AxumAsgiApp


def on_request_done(**event):
    # event keys: method, path, status, duration_s, response_bytes
    print(event)

app = AxumAsgiApp(native_bridge, on_request_done=on_request_done)
```

## Prometheus Integration

Install with the `observability` extra:

```bash
uv sync --extra observability
```

Then use `PrometheusMetricsHook`:

```python
from axum_asgi_bridge import PrometheusMetricsHook

hook = PrometheusMetricsHook(prefix="axum_asgi_bridge")
app = AxumAsgiApp(native_bridge, on_request_done=hook)
```

## Rust-side Tracing

Enable the `observability` feature in Rust to instrument dispatch spans:

```toml
axum_asgi_bridge = { version = "0.1.0", features = ["observability"] }
```

This enables dispatch start/completion tracing events and emits fields such as
method, path, status, and response size.

## Recommended Setup

- Use Python callback hooks for app-level metrics exports.
- Use Rust tracing for dispatch internals and latency drill-down.
- Use both together in production when you need end-to-end visibility.
- Prefer structured logs and a central collector in production deployments.
