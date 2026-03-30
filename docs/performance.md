# Performance

## Zero-JSON Dispatch

The most significant performance optimization in `axum_asgi_bridge` is the
elimination of JSON serialization from the request/response hot path.

### Before (v0.0.x pattern)

```text
Python                              Rust
──────                              ────
dict → json.dumps() → str ────────→ serde_json::from_str() → AsgiHttpScope
                                    ...dispatch...
     str ← json.loads() ←──────── serde_json::to_string() ← Vec<(String, String)>
```

**4 serialization operations per request**: 2 in Python (`json.dumps`, `json.loads`)
and 2 in Rust (`from_str`, `to_string`).

### After (current)

```text
Python                              Rust
──────                              ────
(method, path, qs,    ────────────→ AsgiHttpScope { method, path, ... }
 headers, body)                     (PyO3 native conversion, no parsing)
                                    ...dispatch...
(status, headers,     ←──────────── (status, Vec<(String,String)>, Vec<u8>)
 body)                              (PyO3 native conversion, no serialization)
```

**0 serialization operations per request**. PyO3 converts Python `str` to Rust
`String`, Python `list[tuple[str, str]]` to Rust `Vec<(String, String)>`, and
Python `bytes` to Rust `Vec<u8>` using direct memory operations.

!!! note
  The JSON scope compatibility path has been removed. The bridge uses
  structured dispatch as the single supported request path.

## Body Collection

Response bodies are collected using `http-body-util`'s `BodyExt::collect()`:

```rust
let collected = response.into_body().collect().await?;
let body_bytes: Vec<u8> = collected.to_bytes().into();
```

This is more efficient than manual chunk iteration because:

- **Single-frame responses** (most JSON APIs): `to_bytes()` returns the internal
  buffer directly — **zero copy**.
- **Multi-frame responses**: chunks are concatenated in a pre-sized buffer.
- **`Bytes::into()` → `Vec<u8>`**: when the `Bytes` value owns its buffer
  exclusively, conversion is zero-copy.

## Release Build Profile

The `Cargo.toml` release profile is configured for maximum runtime performance:

```toml
[profile.release]
lto = "fat"            # Cross-crate link-time optimization
codegen-units = 1      # Single codegen unit for better optimization
opt-level = 3          # Maximum optimization level
strip = "symbols"      # Remove debug symbols for smaller binary
```

| Setting | Effect | Trade-off |
|---|---|---|
| Fat LTO | LLVM optimizes across all crate boundaries | Longer compile time (~2-3x) |
| 1 codegen unit | Compiler sees all code at once | Longer compile time |
| opt-level 3 | Aggressive inlining, vectorization, loop unrolling | Slightly larger binary |
| Symbol stripping | 50-80% smaller `.so` / `.pyd` | No debug symbols in releases |

!!! tip
    Always build with `maturin develop --release` or `maturin build --release`.
    Debug builds skip all of these optimizations and run ~10x slower.

## Router Dispatch

Axum's `Router` is `Arc`-backed internally, so `Router::clone()` is an atomic
reference count increment — O(1) regardless of route table size. The bridge
uses `tower::ServiceExt::oneshot()` to dispatch a single request through the
router, which is the canonical pattern for embedded/testing use.

## What the Bridge Does NOT Optimize

These areas are intentionally left simple for clarity:

- **Header value filtering**: Non-UTF-8 header values are silently dropped via
  `filter_map`. This matches common practice but could lose binary header values.
  In HTTP/1.1, header values are restricted to visible ASCII + SP + HTAB, so
  this is rarely an issue.

- **String allocations**: Each header name and value is allocated as a new
  `String`. For typical API responses with 5-15 headers, the allocation overhead
  is negligible compared to network I/O and handler logic.

- **Request body copying**: The request body crosses the Python→Rust boundary
  as `Vec<u8>`, which involves a memory copy from the Python `bytes` object.
  Zero-copy would require holding the GIL across the async boundary, which
  would negate any benefit.

## Benchmarking

To measure bridge overhead, benchmark a minimal handler:

```python
import asyncio
import time
from axum_asgi_bridge import demo_asgi_app

app = demo_asgi_app()

async def bench():
    scope = {"type": "http", "method": "GET", "path": "/",
             "query_string": b"", "headers": []}
    n = 10_000
    body_events = [{"type": "http.request", "body": b"", "more_body": False}]
    responses = []

    async def receive():
        return body_events[0]

    async def send(event):
        if event["type"] == "http.response.body":
            responses.append(event)

    start = time.perf_counter()
    for _ in range(n):
        await app(scope, receive, send)
    elapsed = time.perf_counter() - start
    print(f"{n} requests in {elapsed:.3f}s = {n/elapsed:.0f} req/s")

asyncio.run(bench())
```

Expected bridge overhead is <50µs per request for small JSON responses. The
dominant cost in real applications is handler logic, database queries, and
network I/O — not the bridge.
