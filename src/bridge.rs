use std::collections::BTreeSet;
use std::str::FromStr;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Method, Request, Uri};
use axum::response::Response;
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use tower::ServiceExt;

use crate::error::{BridgeError, Result};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
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
pub struct AxumAsgiBridge {
    router: Router,
    openapi_schema: Option<JsonValue>,
    provided_route_patterns: BTreeSet<String>,
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

    /// Dispatch a request from structured arguments, avoiding JSON overhead.
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

    /// Dispatch a request from a JSON-encoded ASGI scope string.
    pub async fn dispatch_raw(&self, scope_json: &str, body: Vec<u8>) -> Result<DispatchResult> {
        let scope: AsgiHttpScope =
            serde_json::from_str(scope_json).map_err(|error| BridgeError::JsonDecode {
                context: "dispatch_raw.scope",
                message: error.to_string(),
            })?;
        self.dispatch_scope(scope, body).await
    }

    async fn dispatch_scope(&self, scope: AsgiHttpScope, body: Vec<u8>) -> Result<DispatchResult> {
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

        Ok(DispatchResult {
            status,
            headers,
            body: body_bytes,
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
