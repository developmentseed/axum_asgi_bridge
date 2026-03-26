#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets

uv sync --extra dev
uv run maturin develop --release
uv run pytest -v
uv run python examples/openapi_complete_fastapi.py
uv run mkdocs build --strict

echo "check.sh: all checks passed"
