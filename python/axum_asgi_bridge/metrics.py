from __future__ import annotations

from typing import Any


class PrometheusMetricsHook:
    """Record request metrics into prometheus_client counters and histograms."""

    def __init__(
        self,
        prefix: str = "axum_asgi_bridge",
        buckets: tuple[float, ...] | None = None,
    ) -> None:
        try:
            from prometheus_client import Counter, Histogram
        except ImportError as exc:  # pragma: no cover
            raise RuntimeError(
                "prometheus_client is required for PrometheusMetricsHook; install the observability extra"
            ) from exc

        histogram_buckets = buckets or (0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0)
        self._requests = Counter(
            f"{prefix}_requests_total",
            "Total bridge requests",
            ["method", "path", "status"],
        )
        self._latency = Histogram(
            f"{prefix}_request_duration_seconds",
            "Bridge request duration",
            ["method", "path", "status"],
            buckets=histogram_buckets,
        )
        self._response_size = Histogram(
            f"{prefix}_response_bytes",
            "Bridge response size in bytes",
            ["method", "path", "status"],
            buckets=(128, 512, 1024, 4096, 16384, 65536, 262144, 1048576),
        )

    def __call__(self, **kwargs: Any) -> None:
        method = str(kwargs.get("method", "UNKNOWN"))
        path = str(kwargs.get("path", ""))
        status = str(kwargs.get("status", 0))
        duration_s = float(kwargs.get("duration_s", 0.0))
        response_bytes = float(kwargs.get("response_bytes", 0))

        labels = dict(method=method, path=path, status=status)
        self._requests.labels(**labels).inc()
        self._latency.labels(**labels).observe(duration_s)
        self._response_size.labels(**labels).observe(response_bytes)
