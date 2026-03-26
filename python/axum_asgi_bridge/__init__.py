from __future__ import annotations

from ._native import demo_app as _demo_native
from ._native import version
from .asgi import AxumAsgiApp
from .exceptions import (
    BridgeConfigError,
    BridgeDispatchError,
    BridgeError,
    InvalidRequestError,
    ResponseBodyError,
)
from .integrations import (
    DelegatePathsMiddleware,
    install_lifespan,
    install_openapi_merger,
    merge_openapi_with_delegate,
    missing_delegated_routes,
)
from .metrics import PrometheusMetricsHook


def demo_asgi_app() -> AxumAsgiApp:
    """Create a demonstration ASGI app backed by the Rust bridge."""
    return AxumAsgiApp(_demo_native())


__all__ = [
    "AxumAsgiApp",
    "BridgeError",
    "BridgeDispatchError",
    "BridgeConfigError",
    "InvalidRequestError",
    "ResponseBodyError",
    "DelegatePathsMiddleware",
    "PrometheusMetricsHook",
    "demo_asgi_app",
    "install_lifespan",
    "install_openapi_merger",
    "merge_openapi_with_delegate",
    "missing_delegated_routes",
    "version",
]
