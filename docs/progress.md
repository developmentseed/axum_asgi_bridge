# Roadmap Progress

Last updated: 2026-03-26

## Status Summary

1. Streaming response support: Implemented (ASGI chunked send path)
2. WebSocket support: Not started
3. ASGI lifespan events: Implemented
4. Tower middleware integration: Not started
5. Automatic route pattern extraction: Implemented
6. OpenAPI auto-generation (utoipa): Not started
7. Typed Python exception classes: Implemented (pending full test run)
8. Metrics and tracing: Partially implemented (Python hooks + Prometheus; Rust tracing pending)
9. GIL release for synchronous methods: Implemented (pending full test run)

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

## Outstanding Gaps

- Item 1 currently streams from buffered Rust response bytes in Python chunks; true Rust body-frame streaming is still pending.
- Item 8 still needs Rust-side tracing spans / layers to be complete.

## Repeated Mistakes Guardrail

- Always run Rust and Python tests after each roadmap step.
- Always append blockers immediately when encountered; do not retry the same broken approach more than twice.
- Keep commits scoped to one or two roadmap items each.
