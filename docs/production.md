# Production Guide

This page summarizes deployment guidance and operational constraints.

## Build and Release

- Use release builds only:

```bash
uv run maturin build --release --out dist
```

- Keep CI strict:

```bash
scripts/check.sh
```

- Pin and review dependency updates (`Cargo.lock`, `uv.lock`) in pull requests.

## Runtime Configuration

### Rust features

- `middleware`: compression, CORS, timeout, and trace layer helpers
- `observability`: dispatch tracing events
- `utoipa`: OpenAPI schema helper from a utoipa document

### ASGI integration

- Use `DelegatePathsMiddleware` for route precedence control.
- Use `install_openapi_merger` when delegated routes are not mounted directly.
- Use `install_lifespan` to call delegated startup/shutdown hooks.

## Throughput and Backpressure

- Prefer native `dispatch_to_send` for HTTP response streaming because each
  ASGI send call is awaited before the next body frame is forwarded.
- `dispatch_streaming` remains useful for compatibility and testing but returns
  chunk vectors.

## WebSocket Behavior

- The bridge implements a Rust-native ASGI websocket protocol loop
  (connect/accept/receive/send/disconnect/close).
- Axum extractor-native websocket upgrades are not available through the
  current `Router::oneshot` embedding path because upgrade context is not
  present.

## Failure Handling

Use typed exceptions from the package:

- `BridgeError`
- `BridgeDispatchError`
- `BridgeConfigError`
- `InvalidRequestError`
- `ResponseBodyError`

These support explicit error handling instead of matching generic runtime errors.

## Operational Checklist

- Confirm release binary/wheel artifacts in staging.
- Validate large body responses and websocket traffic patterns.
- Verify timeout values and cancellation behavior under load.
- Confirm metrics and tracing integration before production rollout.
