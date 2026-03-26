# CI and Release

## CI Workflows

The CI pipeline runs three stages:

### Rust checks

```bash
cargo fmt --check          # Code formatting
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo test --all-targets   # Unit and integration tests
```

### Python checks

```bash
uv sync --extra dev                # Install dependencies
uv run maturin develop --release   # Build native extension
uv run pytest -v                   # Integration tests
uv run python examples/openapi_complete_fastapi.py  # OpenAPI completeness
```

### Documentation

```bash
uv run mkdocs build --strict   # Build docs, fail on warnings
```

Run the full pipeline locally with:

```bash
scripts/check.sh
```

## Wheel Build

The `wheels.yml` workflow builds platform wheels for:

| Platform | Architectures |
|---|---|
| Linux | x86_64, aarch64 |
| macOS | x86_64, aarch64 (Apple Silicon) |
| Windows | x86_64 |

Wheels are uploaded as workflow artifacts and can be downloaded from the
Actions tab.

Build wheels locally:

```bash
scripts/build_wheels_local.sh
# Wheels are in dist/
```

## Release Process

1. Update the version in both `Cargo.toml` and `pyproject.toml`.
2. Tag the release: `git tag v0.1.0 && git push --tags`.
3. The wheel workflow runs automatically on tags.
4. Download and verify wheel artifacts.
5. Publish to PyPI using trusted publishing or an API token:

```bash
uv run twine upload dist/*.whl
```

## Release Profile

Release builds are optimized with:

| Setting | Value | Effect |
|---|---|---|
| `lto` | `"fat"` | Cross-crate link-time optimization for smaller, faster binaries |
| `codegen-units` | `1` | Better optimization at the cost of longer compile times |
| `opt-level` | `3` | Maximum runtime performance |
| `strip` | `"symbols"` | Removes debug symbols, reducing binary size |
