use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("invalid HTTP method: {0}")]
    InvalidMethod(String),

    #[error("invalid URI path/query: {0}")]
    InvalidUri(String),

    #[error("invalid header name: {0}")]
    InvalidHeaderName(String),

    #[error("invalid header value for {name}: {message}")]
    InvalidHeaderValue { name: String, message: String },

    #[error("service dispatch failed: {0}")]
    Service(String),

    #[error("response body read failed: {0}")]
    ResponseBody(String),

    #[error("JSON encoding failed ({context}): {message}")]
    JsonEncode {
        context: &'static str,
        message: String,
    },
}

pub type Result<T> = std::result::Result<T, BridgeError>;
