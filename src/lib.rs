mod bridge;
mod error;

use std::collections::HashMap;

use axum::routing::{get, post};
use axum::Json;
use futures_util::StreamExt;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::{Value as JsonValue, json};

pub use bridge::{
    AsgiHttpScope, AxumAsgiBridge, DispatchResult, DispatchStreamingResult, RouteRegistry,
};
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

async fn await_python(send_or_receive: &Py<PyAny>, args: Py<PyAny>) -> PyResult<Py<PyAny>> {
    let future = Python::attach(|py| {
        let awaitable = send_or_receive.bind(py).call1((args.bind(py),))?;
        pyo3_async_runtimes::tokio::into_future(awaitable)
    })?;
    future.await
}

async fn await_receive_event(receive: &Py<PyAny>) -> PyResult<Py<PyAny>> {
    let future = Python::attach(|py| {
        let awaitable = receive.bind(py).call0()?;
        pyo3_async_runtimes::tokio::into_future(awaitable)
    })?;
    future.await
}

async fn send_http_start(send: &Py<PyAny>, status: u16, headers: &[(String, String)]) -> PyResult<()> {
    let event: Py<PyAny> = Python::attach(|py| -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("type", "http.response.start")?;
        dict.set_item("status", status)?;

        let header_list: Vec<(Vec<u8>, Vec<u8>)> = headers
            .iter()
            .map(|(name, value)| (name.as_bytes().to_vec(), value.as_bytes().to_vec()))
            .collect();
        dict.set_item("headers", header_list)?;
        Ok(dict.into_any().unbind())
    })?;
    let _ = await_python(send, event).await?;
    Ok(())
}

async fn send_http_body(send: &Py<PyAny>, body: &[u8], more_body: bool) -> PyResult<()> {
    let event: Py<PyAny> = Python::attach(|py| -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("type", "http.response.body")?;
        dict.set_item("body", PyBytes::new(py, body))?;
        dict.set_item("more_body", more_body)?;
        Ok(dict.into_any().unbind())
    })?;
    let _ = await_python(send, event).await?;
    Ok(())
}

async fn send_ws_event(send: &Py<PyAny>, event_type: &str, text: Option<&str>, bytes: Option<&[u8]>) -> PyResult<()> {
    let event: Py<PyAny> = Python::attach(|py| -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("type", event_type)?;
        if let Some(value) = text {
            dict.set_item("text", value)?;
        }
        if let Some(value) = bytes {
            dict.set_item("bytes", PyBytes::new(py, value))?;
        }
        Ok(dict.into_any().unbind())
    })?;
    let _ = await_python(send, event).await?;
    Ok(())
}

#[derive(Clone)]
#[pyclass(skip_from_py_object)]
pub struct PyAxumAsgiBridge {
    inner: AxumAsgiBridge,
}

#[pymethods]
impl PyAxumAsgiBridge {
    #[cfg(feature = "middleware")]
    fn with_compression(&self) -> Self {
        Self {
            inner: self.inner.clone().with_compression(),
        }
    }

    #[cfg(feature = "middleware")]
    fn with_cors_permissive(&self) -> Self {
        Self {
            inner: self.inner.clone().with_cors_permissive(),
        }
    }

    #[cfg(feature = "middleware")]
    fn with_timeout_millis(&self, timeout_millis: u64) -> Self {
        Self {
            inner: self
                .inner
                .clone()
                .with_timeout(std::time::Duration::from_millis(timeout_millis)),
        }
    }

    #[cfg(feature = "middleware")]
    fn with_trace_http(&self) -> Self {
        Self {
            inner: self.inner.clone().with_trace_http(),
        }
    }

    /// Dispatch from structured arguments.
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

    /// Dispatch from a JSON scope payload (compatibility path).
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

    /// Dispatch and return body chunks as vectors.
    fn dispatch_streaming<'py>(
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
                .dispatch_streaming(method, path, query_string, headers, body)
                .await
                .map_err(to_py_err)?;
            Ok((result.status, result.headers, result.chunks))
        })
    }

    /// Dispatch and forward frames to ASGI send, awaiting each send call.
    fn dispatch_to_send<'py>(
        &self,
        py: Python<'py>,
        method: String,
        path: String,
        query_string: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        send: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let send = send.unbind();
        pyo3_async_runtimes::tokio::future_into_py::<_, ()>(py, async move {
            let response = inner
                .dispatch_response(method, path, query_string, headers, body)
                .await
                .map_err(to_py_err)?;

            let status = response.status().as_u16();
            let response_headers: Vec<(String, String)> = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|v| (name.as_str().to_owned(), v.to_owned()))
                })
                .collect();

            send_http_start(&send, status, &response_headers).await?;

            let mut stream = response.into_body().into_data_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| ResponseBodyErrorPy::new_err(error.to_string()))?;
                send_http_body(&send, &chunk, true).await?;
            }
            send_http_body(&send, &[], false).await?;
            Ok(())
        })
    }

    fn openapi_schema_json(&self, py: Python<'_>) -> PyResult<Option<String>> {
        // Only Rust-owned data is touched, so this can run detached from the GIL.
        py.detach(|| self.inner.openapi_schema_json())
            .map_err(to_py_err)
    }

    fn provided_route_patterns_json(&self, py: Python<'_>) -> PyResult<String> {
        // Only Rust-owned data is touched, so this can run detached from the GIL.
        py.detach(|| self.inner.provided_route_patterns_json())
            .map_err(to_py_err)
    }

    fn on_startup<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    fn on_shutdown<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move { Ok(()) })
    }

    fn dispatch_websocket<'py>(
        &self,
        py: Python<'py>,
        _scope: Bound<'py, PyAny>,
        receive: Bound<'py, PyAny>,
        send: Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let receive = receive.unbind();
        let send = send.unbind();
        pyo3_async_runtimes::tokio::future_into_py::<_, ()>(py, async move {
            let connect_event = await_receive_event(&receive).await?;
            let connect_kind = Python::attach(|py| {
                connect_event
                    .bind(py)
                    .call_method1("get", ("type",))?
                    .extract::<String>()
            })?;
            if connect_kind != "websocket.connect" {
                return Err(InvalidRequestErrorPy::new_err(format!(
                    "expected websocket.connect, got {connect_kind}"
                )));
            }

            send_ws_event(&send, "websocket.accept", None, None).await?;

            loop {
                let event = await_receive_event(&receive).await?;
                let event_kind = Python::attach(|py| {
                    event
                        .bind(py)
                        .call_method1("get", ("type",))?
                        .extract::<String>()
                })?;

                if event_kind == "websocket.disconnect" {
                    break;
                }

                if event_kind == "websocket.receive" {
                    let (text, bytes): (Option<String>, Option<Vec<u8>>) = Python::attach(|py| {
                        let event = event.bind(py);
                        let text_value = event.call_method1("get", ("text",))?;
                        let text = if text_value.is_none() {
                            None
                        } else {
                            Some(text_value.extract::<String>()?)
                        };

                        let bytes_value = event.call_method1("get", ("bytes",))?;
                        let bytes = if bytes_value.is_none() {
                            None
                        } else {
                            Some(bytes_value.extract::<Vec<u8>>()?)
                        };
                        Ok::<_, PyErr>((text, bytes))
                    })?;

                    if let Some(text) = text.as_deref() {
                        send_ws_event(&send, "websocket.send", Some(text), None).await?;
                    }
                    if let Some(bytes) = bytes.as_deref() {
                        send_ws_event(&send, "websocket.send", None, Some(bytes)).await?;
                    }
                    continue;
                }

                if event_kind == "websocket.close" {
                    send_ws_event(&send, "websocket.close", None, None).await?;
                    break;
                }
            }
            Ok(())
        })
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
