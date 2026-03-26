mod bridge;
mod error;

use std::collections::HashMap;

use axum::routing::{get, post};
use axum::Json;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use serde_json::{Value as JsonValue, json};

pub use bridge::{AsgiHttpScope, AxumAsgiBridge, DispatchResult, RouteRegistry};
pub use error::{BridgeError, Result};

create_exception!(_native, BridgeErrorPy, PyException);
create_exception!(_native, BridgeDispatchErrorPy, BridgeErrorPy);
create_exception!(_native, BridgeConfigErrorPy, BridgeErrorPy);
create_exception!(_native, InvalidRequestErrorPy, BridgeErrorPy);
create_exception!(_native, ResponseBodyErrorPy, BridgeErrorPy);

fn to_py_err(error: BridgeError) -> PyErr {
    let message = error.to_string();
    match error {
        BridgeError::JsonDecode { .. } => BridgeConfigErrorPy::new_err(message),
        BridgeError::JsonEncode { .. } => BridgeConfigErrorPy::new_err(message),
        BridgeError::InvalidMethod(_)
        | BridgeError::InvalidUri(_)
        | BridgeError::InvalidHeaderName(_)
        | BridgeError::InvalidHeaderValue { .. } => InvalidRequestErrorPy::new_err(message),
        BridgeError::Service(_) => BridgeDispatchErrorPy::new_err(message),
        BridgeError::ResponseBody(_) => ResponseBodyErrorPy::new_err(message),
    }
}

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
                .map_err(to_py_err)?;
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
                .map_err(to_py_err)?;
            let headers_json = serde_json::to_string(&result.headers)
                .map_err(|e| BridgeConfigErrorPy::new_err(e.to_string()))?;
            Ok((result.status, headers_json, result.body))
        })
    }

    fn openapi_schema_json(&self, py: Python<'_>) -> PyResult<Option<String>> {
        // This function only touches Rust-owned data, so running it without the GIL is safe.
        py.detach(|| self.inner.openapi_schema_json())
            .map_err(to_py_err)
    }

    fn provided_route_patterns_json(&self, py: Python<'_>) -> PyResult<String> {
        // This function only touches Rust-owned data, so running it without the GIL is safe.
        py.detach(|| self.inner.provided_route_patterns_json())
            .map_err(to_py_err)
    }

    fn on_startup<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    fn on_shutdown<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }
}

fn demo_routes() -> RouteRegistry {
    async fn health() -> Json<JsonValue> {
        Json(json!({"ok": true, "service": "axum_asgi_bridge"}))
    }

    async fn echo(body: String) -> Json<JsonValue> {
        Json(json!({"echo": body}))
    }

    RouteRegistry::new()
        .route("/", get(health))
        .route("/echo", post(echo))
}

#[pyfunction]
#[pyo3(signature = (_config = None))]
fn demo_app(_config: Option<HashMap<String, String>>) -> PyAxumAsgiBridge {
    let bridge = demo_routes()
        .into_bridge()
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
    m.add("BridgeError", _py.get_type::<BridgeErrorPy>())?;
    m.add("BridgeDispatchError", _py.get_type::<BridgeDispatchErrorPy>())?;
    m.add("BridgeConfigError", _py.get_type::<BridgeConfigErrorPy>())?;
    m.add("InvalidRequestError", _py.get_type::<InvalidRequestErrorPy>())?;
    m.add("ResponseBodyError", _py.get_type::<ResponseBodyErrorPy>())?;
    m.add_function(wrap_pyfunction!(demo_app, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
