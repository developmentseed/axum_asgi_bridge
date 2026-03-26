# Middleware

`axum_asgi_bridge` supports feature-gated Tower HTTP middleware builders.

Enable the feature in `Cargo.toml`:

```toml
axum_asgi_bridge = { version = "0.1.0", features = ["middleware"] }
```

## Rust Builders

When `middleware` is enabled, `AxumAsgiBridge` supports:

- `with_compression()`
- `with_cors_permissive()`
- `with_timeout(Duration)`
- `with_trace_http()`

Example:

```rust
use std::time::Duration;
use axum_asgi_bridge::AxumAsgiBridge;

let bridge = AxumAsgiBridge::new(router)
    .with_compression()
    .with_cors_permissive()
    .with_timeout(Duration::from_secs(5))
    .with_trace_http();
```

## Python Convenience Methods

When compiled with the `middleware` feature, `PyAxumAsgiBridge` also exposes:

- `.with_compression()`
- `.with_cors_permissive()`
- `.with_timeout_millis(ms)`
- `.with_trace_http()`

These return cloned bridge instances with the layer applied.

## Notes

- Middleware support is feature-gated to keep the default build lightweight.
- `with_trace_http()` requires request logging setup in your Rust runtime.
