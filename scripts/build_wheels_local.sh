#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

uv sync --extra dev
uv run maturin build --release --out dist

echo "Wheels/sdist are in dist/"
