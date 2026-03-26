mod bridge;
mod error;

use std::collections::HashMap;

use axum::routing::{get, post};
use axum::{Json, Router};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde_json::{Value as JsonValue, json};

pub use bridge::{AsgiHttpScope, AxumAsgiBridge, DispatchResult};
pub use error::{BridgeError, Result};

#[derive(Clone)]
#[pyclass(skip_from_py_object)]
pub struct PyAxumAsgiBridge {
    inner: AxumAsgiBridge,
}

#[pymethods]
impl PyAxumAsgiBridge {
    /// Dispatch via structured arguments — no JSON serialization overhead.
    fn dispatch<'py>(
        &self,
        py: Python<'py>,
        method: String,
        path: String,
        query_string: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = inner
                .dispatch(method, path, query_string, headers, body)
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok((result.status, result.headers, result.body))
        })
    }

    /// Dispatch via JSON scope string — kept for backward compatibility.
    fn dispatch_bytes<'py>(
        &self,
        py: Python<'py>,
        scope_json: String,
        body: Vec<u8>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let result = inner
                .dispatch_raw(&scope_json, body)
                .await
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            let headers_json = serde_json::to_string(&result.headers)
                .map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
            Ok((result.status, headers_json, result.body))
        })
    }

    fn openapi_schema_json(&self) -> PyResult<Option<String>> {
        self.inner
            .openapi_schema_json()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }

    fn provided_route_patterns_json(&self) -> PyResult<String> {
        self.inner
            .provided_route_patterns_json()
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
    }
}

fn demo_router() -> Router {
    async fn health() -> Json<JsonValue> {
        Json(json!({"ok": true, "service": "axum_asgi_bridge"}))
    }

    async fn echo(body: String) -> Json<JsonValue> {
        Json(json!({"echo": body}))
    }

    Router::new()
        .route("/", get(health))
        .route("/echo", post(echo))
}

#[pyfunction]
#[pyo3(signature = (_config = None))]
fn demo_app(_config: Option<HashMap<String, String>>) -> PyAxumAsgiBridge {
    let bridge = AxumAsgiBridge::new(demo_router())
        .with_route_patterns(["/".to_string(), "/echo".to_string()])
        .with_openapi_schema(json!({
            "openapi": "3.1.0",
            "info": {"title": "axum_asgi_bridge demo", "version": "0.1.0"},
            "paths": {
                "/": {"get": {"responses": {"200": {"description": "ok"}}}},
                "/echo": {"post": {"responses": {"200": {"description": "echo"}}}}
            }
        }));
    PyAxumAsgiBridge { inner: bridge }
}

#[pyfunction]
fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[pymodule]
fn _native(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAxumAsgiBridge>()?;
    m.add_function(wrap_pyfunction!(demo_app, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
