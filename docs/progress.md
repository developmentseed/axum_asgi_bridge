# Roadmap Progress

Last updated: 2026-03-26

## Status Summary

1. Streaming response support: Not started
2. WebSocket support: Not started
3. ASGI lifespan events: Not started
4. Tower middleware integration: Not started
5. Automatic route pattern extraction: Not started
6. OpenAPI auto-generation (utoipa): Not started
7. Typed Python exception classes: Implemented (pending full test run)
8. Metrics and tracing: Not started
9. GIL release for synchronous methods: Implemented (pending full test run)

## Execution Log

- 2026-03-26: Created progress tracker and established clean `main` baseline.
- 2026-03-26: Starting implementation for items 7 and 9 in first coding pass.
- 2026-03-26: Added typed native exceptions (`BridgeError`, `BridgeDispatchError`, `BridgeConfigError`, `InvalidRequestError`, `ResponseBodyError`) and mapped Rust `BridgeError` variants to them.
- 2026-03-26: Added GIL-release wrappers (`py.allow_threads`) for `openapi_schema_json` and `provided_route_patterns_json`.
- 2026-03-26: Added tests `test_exceptions.py` and `test_threadsafety.py`.
- 2026-03-26: Blocker found during validation: PyO3 `0.28` in this project does not provide `Python::allow_threads`; switched implementation to `py.detach(...)` and resumed testing.
- 2026-03-26: Test issue found: method `NOPE` is syntactically valid in HTTP, so `InvalidMethod` was not raised; updated exception test to use invalid header name (`"bad header"`) for deterministic `InvalidRequestError` coverage.

## Repeated Mistakes Guardrail

- Always run Rust and Python tests after each roadmap step.
- Always append blockers immediately when encountered; do not retry the same broken approach more than twice.
- Keep commits scoped to one or two roadmap items each.
