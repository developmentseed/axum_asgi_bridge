use std::collections::BTreeSet;
use std::str::FromStr;
#[cfg(feature = "middleware")]
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::routing::MethodRouter;
use axum::http::{HeaderName, HeaderValue, Method, Request, Uri};
#[cfg(feature = "middleware")]
use axum::http::StatusCode;
use axum::response::Response;
use futures_util::StreamExt;
use http_body_util::BodyExt;
use serde_json::Value as JsonValue;
use tower::ServiceExt;

#[cfg(feature = "middleware")]
use tower_http::compression::CompressionLayer;
#[cfg(feature = "middleware")]
use tower_http::cors::CorsLayer;
#[cfg(feature = "middleware")]
use tower_http::timeout::TimeoutLayer;
#[cfg(feature = "middleware")]
use tower_http::trace::TraceLayer;

use crate::error::{BridgeError, Result};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AsgiHttpScope {
    pub method: String,
    pub path: String,
    pub query_string: Option<String>,
    pub headers: Vec<(String, String)>,
}

#[derive(Clone, Debug)]
pub struct DispatchResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct DispatchStreamingResult {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub chunks: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct AxumAsgiBridge {
    router: Router,
    openapi_schema: Option<JsonValue>,
    provided_route_patterns: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RouteRegistry {
    router: Router,
    patterns: Vec<String>,
}

impl RouteRegistry {
    pub fn new() -> Self {
        Self {
            router: Router::new(),
            patterns: Vec::new(),
        }
    }

    pub fn route(mut self, path: &str, method_router: MethodRouter) -> Self {
        self.patterns.push(path.to_owned());
        self.router = self.router.route(path, method_router);
        self
    }

    pub fn nest(mut self, prefix: &str, nested: RouteRegistry) -> Self {
        let normalized = if prefix.ends_with('/') {
            prefix.trim_end_matches('/').to_owned()
        } else {
            prefix.to_owned()
        };
        for pattern in nested.patterns {
            let joined = if normalized.is_empty() {
                pattern
            } else {
                format!("{}{}", normalized, pattern)
            };
            self.patterns.push(joined);
        }
        self.router = self.router.nest(prefix, nested.router);
        self
    }

    pub fn merge(mut self, other: RouteRegistry) -> Self {
        self.patterns.extend(other.patterns);
        self.router = self.router.merge(other.router);
        self
    }

    pub fn into_bridge(self) -> AxumAsgiBridge {
        AxumAsgiBridge::new(self.router).with_route_patterns(self.patterns)
    }
}

impl AxumAsgiBridge {
    pub fn new(router: Router) -> Self {
        Self {
            router,
            openapi_schema: None,
            provided_route_patterns: BTreeSet::new(),
        }
    }

    pub fn with_openapi_schema(mut self, schema: JsonValue) -> Self {
        self.openapi_schema = Some(schema);
        self
    }

    pub fn with_route_patterns(mut self, patterns: impl IntoIterator<Item = String>) -> Self {
        self.provided_route_patterns = patterns.into_iter().collect();
        self
    }

    #[cfg(feature = "middleware")]
    pub fn with_compression(mut self) -> Self {
        self.router = self.router.layer(CompressionLayer::new());
        self
    }

    #[cfg(feature = "middleware")]
    pub fn with_cors_permissive(mut self) -> Self {
        self.router = self.router.layer(CorsLayer::permissive());
        self
    }

    #[cfg(feature = "middleware")]
    pub fn with_timeout(mut self, duration: Duration) -> Self {
        self.router = self.router.layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            duration,
        ));
        self
    }

    #[cfg(feature = "middleware")]
    pub fn with_trace_http(mut self) -> Self {
        self.router = self.router.layer(TraceLayer::new_for_http());
        self
    }

    #[cfg(feature = "utoipa")]
    pub fn with_utoipa_schema<A>(self) -> Self
    where
        A: utoipa::OpenApi,
    {
        let schema = serde_json::to_value(A::openapi()).unwrap_or(JsonValue::Null);
        self.with_openapi_schema(schema)
    }

    pub fn openapi_schema_json(&self) -> Result<Option<String>> {
        self.openapi_schema
            .as_ref()
            .map(|schema| {
                serde_json::to_string(schema).map_err(|error| BridgeError::JsonEncode {
                    context: "openapi_schema_json",
                    message: error.to_string(),
                })
            })
            .transpose()
    }

    pub fn provided_route_patterns_json(&self) -> Result<String> {
        let routes: Vec<String> = self.provided_route_patterns.iter().cloned().collect();
        serde_json::to_string(&routes).map_err(|error| BridgeError::JsonEncode {
            context: "provided_route_patterns_json",
            message: error.to_string(),
        })
    }

    /// Dispatch a request from structured arguments.
    pub async fn dispatch(
        &self,
        method: String,
        path: String,
        query_string: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<DispatchResult> {
        let scope = AsgiHttpScope {
            method,
            path,
            query_string: if query_string.is_empty() {
                None
            } else {
                Some(query_string)
            },
            headers,
        };
        self.dispatch_scope(scope, body).await
    }

    /// Dispatch a request and return body chunks.
    pub async fn dispatch_streaming(
        &self,
        method: String,
        path: String,
        query_string: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<DispatchStreamingResult> {
        let scope = AsgiHttpScope {
            method,
            path,
            query_string: if query_string.is_empty() {
                None
            } else {
                Some(query_string)
            },
            headers,
        };
        self.dispatch_scope_streaming(scope, body).await
    }

    /// Dispatch and return a raw HTTP response for caller-managed streaming.
    pub async fn dispatch_response(
        &self,
        method: String,
        path: String,
        query_string: String,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Result<Response> {
        let scope = AsgiHttpScope {
            method,
            path,
            query_string: if query_string.is_empty() {
                None
            } else {
                Some(query_string)
            },
            headers,
        };

        #[cfg(feature = "observability")]
        tracing::info!(
            http.method = %scope.method,
            http.path = %scope.path,
            "dispatch start"
        );

        let request = scope_to_http_request(scope, body)?;
        self.router
            .clone()
            .oneshot(request)
            .await
            .map_err(|error| BridgeError::Service(error.to_string()))
    }

    async fn dispatch_scope(&self, scope: AsgiHttpScope, body: Vec<u8>) -> Result<DispatchResult> {
        #[cfg(feature = "observability")]
        tracing::info!(
            http.method = %scope.method,
            http.path = %scope.path,
            "dispatch start"
        );

        let request = scope_to_http_request(scope, body)?;
        let response: Response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .map_err(|error| BridgeError::Service(error.to_string()))?;

        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_owned(), v.to_owned()))
            })
            .collect();

        let collected = response
            .into_body()
            .collect()
            .await
            .map_err(|error| BridgeError::ResponseBody(error.to_string()))?;
        let body_bytes: Vec<u8> = collected.to_bytes().into();

        #[cfg(feature = "observability")]
        tracing::info!(http.status = status, response.bytes = body_bytes.len(), "dispatch complete");

        Ok(DispatchResult {
            status,
            headers,
            body: body_bytes,
        })
    }

    async fn dispatch_scope_streaming(
        &self,
        scope: AsgiHttpScope,
        body: Vec<u8>,
    ) -> Result<DispatchStreamingResult> {
        let request = scope_to_http_request(scope, body)?;
        let response: Response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .map_err(|error| BridgeError::Service(error.to_string()))?;

        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_owned(), v.to_owned()))
            })
            .collect();

        let mut stream = response.into_body().into_data_stream();
        let mut chunks: Vec<Vec<u8>> = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| BridgeError::ResponseBody(error.to_string()))?;
            chunks.push(chunk.to_vec());
        }

        Ok(DispatchStreamingResult {
            status,
            headers,
            chunks,
        })
    }
}

fn scope_to_http_request(scope: AsgiHttpScope, body: Vec<u8>) -> Result<Request<Body>> {
    let method = Method::from_str(&scope.method)
        .map_err(|_| BridgeError::InvalidMethod(scope.method.clone()))?;

    let query = scope.query_string.unwrap_or_default();
    let uri_text = if query.is_empty() {
        scope.path
    } else {
        format!("{}?{}", scope.path, query)
    };
    let uri = Uri::from_str(&uri_text).map_err(|_| BridgeError::InvalidUri(uri_text))?;

    let mut builder = Request::builder().method(method).uri(uri);
    {
        let headers_map = builder
            .headers_mut()
            .ok_or_else(|| BridgeError::Service("unable to mutate request headers".to_string()))?;
        for (name, value) in scope.headers {
            let header_name = HeaderName::from_str(&name)
                .map_err(|_| BridgeError::InvalidHeaderName(name.clone()))?;
            let header_value =
                HeaderValue::from_str(&value).map_err(|error| BridgeError::InvalidHeaderValue {
                    name: name.clone(),
                    message: error.to_string(),
                })?;
            headers_map.insert(header_name, header_value);
        }
    }
    builder
        .body(Body::from(body))
        .map_err(|error| BridgeError::Service(error.to_string()))
}
