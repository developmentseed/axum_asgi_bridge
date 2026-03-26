from __future__ import annotations

from ._native import demo_app as _demo_native
from ._native import version
from .asgi import AxumAsgiApp
from .integrations import (
    DelegatePathsMiddleware,
    install_openapi_merger,
    merge_openapi_with_delegate,
    missing_delegated_routes,
)


def demo_asgi_app() -> AxumAsgiApp:
    """Create a demonstration ASGI app backed by the Rust bridge."""
    return AxumAsgiApp(_demo_native())


__all__ = [
    "AxumAsgiApp",
    "DelegatePathsMiddleware",
    "demo_asgi_app",
    "install_openapi_merger",
    "merge_openapi_with_delegate",
    "missing_delegated_routes",
    "version",
]
