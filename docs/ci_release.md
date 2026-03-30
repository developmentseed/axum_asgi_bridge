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

The `wheels.yml` workflow builds wheels and sdist artifacts for:

| Platform | Architectures |
|---|---|
| Linux | x86_64, aarch64 |
| macOS | x86_64, aarch64 |
| Windows | x86_64 |

Artifacts are uploaded in every tag-triggered run and include one wheel set per
target plus an sdist archive.

Build wheels locally:

```bash
scripts/build_wheels_local.sh
# Wheels are in dist/
```

## Release Process

1. Update the version in both `Cargo.toml` and `pyproject.toml`.
2. Publish a GitHub release (tag should match `v*`).
3. `wheels.yml` builds cross-platform wheels and sdist artifacts.
4. On release publication, CI publishes Python artifacts to PyPI via trusted publishing.
5. On release publication, CI publishes the Rust crate to crates.io using `CRATES_IO_TOKEN`.

Required repository configuration:

- Configure PyPI trusted publishing for this repository.
- Add repository secret `CRATES_IO_TOKEN` with publish permissions.

## Release Profile

Release builds are optimized with:

| Setting | Value | Effect |
|---|---|---|
| `lto` | `"fat"` | Cross-crate link-time optimization for smaller, faster binaries |
| `codegen-units` | `1` | Better optimization at the cost of longer compile times |
| `opt-level` | `3` | Maximum runtime performance |
| `strip` | `"symbols"` | Removes debug symbols, reducing binary size |
