# Roadmap Progress

Last updated: 2026-03-26

## Status Summary

1. Streaming response support: Implemented (native chunk path + ASGI chunk sends)
2. WebSocket support: Implemented protocol loop (echo bridge); Axum websocket-upgrade integration still pending
3. ASGI lifespan events: Implemented
4. Tower middleware integration: Implemented (feature-gated)
5. Automatic route pattern extraction: Implemented
6. OpenAPI auto-generation (utoipa): Implemented (feature-gated helper + tests + CI)
7. Typed Python exception classes: Implemented and validated
8. Metrics and tracing: Implemented (Python hooks + Prometheus + Rust tracing events)
9. GIL release for synchronous methods: Implemented and validated

## Execution Log

- 2026-03-26: Created progress tracker and established clean `main` baseline.
- 2026-03-26: Starting implementation for items 7 and 9 in first coding pass.
- 2026-03-26: Added typed native exceptions (`BridgeError`, `BridgeDispatchError`, `BridgeConfigError`, `InvalidRequestError`, `ResponseBodyError`) and mapped Rust `BridgeError` variants to them.
- 2026-03-26: Added GIL-release wrappers (`py.allow_threads`) for `openapi_schema_json` and `provided_route_patterns_json`.
- 2026-03-26: Added tests `test_exceptions.py` and `test_threadsafety.py`.
- 2026-03-26: Blocker found during validation: PyO3 `0.28` in this project does not provide `Python::allow_threads`; switched implementation to `py.detach(...)` and resumed testing.
- 2026-03-26: Test issue found: method `NOPE` is syntactically valid in HTTP, so `InvalidMethod` was not raised; updated exception test to use invalid header name (`"bad header"`) for deterministic `InvalidRequestError` coverage.
- 2026-03-26: Started item 5 implementation with new `RouteRegistry` type and migrated demo app to route registration through the registry.
- 2026-03-26: Completed item 5 with tests (`route_registry_tracks_patterns`); fixed follow-up warning by removing an unused `Router` import.
- 2026-03-26: Started integrated pass for items 1, 3, and 8: added ASGI lifespan handling, chunked response send path, request timing hooks, and Prometheus hook module.
- 2026-03-26: Added `install_lifespan` integration helper and lifecycle passthrough behavior in middleware.
- 2026-03-26: Added tests for lifespan events, chunked body sends, request metrics callback payload, and FastAPI lifespan installation.
- 2026-03-26: Validation passed after this pass (`cargo test --all-targets`, `uv run pytest -q` with 11 tests).
- 2026-03-26: Started Rust-side pass for items 4/6/8: added feature flags and optional deps for `tower-http`, `utoipa`, and `tracing`.
- 2026-03-26: Added middleware and observability docs pages plus a utoipa integration example scaffold.
- 2026-03-26: Blocker found during `--all-features` tests: `EnteredSpan` is not `Send` across async await in PyO3 futures. Resolved by switching to start/end `tracing::info!` events instead of holding entered span guards.
- 2026-03-26: Blocker found in middleware timeout builder: `TimeoutLayer::with_status_code` argument order differs in `tower-http 0.6`; corrected to `(status_code, duration)`.
- 2026-03-26: Item 1 follow-up: added native `dispatch_streaming` / `dispatch_raw_streaming` paths to preserve body chunk boundaries from Rust to Python.
- 2026-03-26: Item 2 pass: added websocket dispatch call path in Python ASGI adapter and native placeholder method; full Rust websocket upgrade bridge remains outstanding.
- 2026-03-26: Fixed websocket placeholder compile issue by explicitly setting `future_into_py::<_, ()>` return type in PyO3 binding.
- 2026-03-26: Final validation checkpoint: `cargo test --all-targets`, `cargo test --all-targets --all-features`, `uv run pytest -q` (13 passed), and `uv run mkdocs build --strict` all pass.
- 2026-03-26: New pass started to complete the two remaining gaps: full websocket protocol loop and fully backpressure-driven streaming handoff.
- 2026-03-26: Added bridge-level `dispatch_response` API to support no-buffer body forwarding from Rust to ASGI `send`.
- 2026-03-26: Integrated native `dispatch_to_send` into Python ASGI adapter for backpressure-first HTTP path (awaiting each send event, no chunk vector materialization in Python).
- 2026-03-26: Blocker found in PyO3 event extraction (dict item API differences); fixed by using Python `dict.get(...)` calls and explicit closure return typing in native async send/receive helpers.
- 2026-03-26: Completed native websocket protocol loop in Rust (connect/accept, receive echo for text/bytes, disconnect/close handling) and added direct native protocol tests.
- 2026-03-26: Updated backpressure dispatch callback accounting to capture real response status from `http.response.start` events.
- 2026-03-26: Validation checkpoint after websocket/backpressure completion: `cargo test --all-targets`, `cargo test --all-targets --all-features`, `uv run pytest -q` (15 passed), and `uv run mkdocs build --strict` all pass.

## Outstanding Gaps

- Item 2 Axum-native websocket upgrade integration is still pending; current protocol bridge is Rust-native ASGI websocket echo loop (connect/accept/receive/send/close/disconnect), not route-dispatched via Axum upgrade extractors.
- Item 1 native backpressure handoff is implemented through `dispatch_to_send`; existing `dispatch_streaming` vector-chunk API remains for compatibility.

## Repeated Mistakes Guardrail

- Always run Rust and Python tests after each roadmap step.
- Always append blockers immediately when encountered; do not retry the same broken approach more than twice.
- Keep commits scoped to one or two roadmap items each.
