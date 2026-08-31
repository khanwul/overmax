//! Gateway domain error types.

use std::fmt;

pub type GatewayResult<T> = Result<T, GatewayError>;

#[derive(Debug)]
pub enum GatewayError {
    Network(reqwest::Error),
    Io(std::io::Error),
    Json(serde_json::Error),
    InvalidProtocol {
        expected: &'static str,
        actual: String,
    },
    HttpError {
        status: u16,
        message: String,
    },
    AssetNotFound(String),
    Custom(String),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Network(e) => write!(f, "네트워크 오류: {e}"),
            Self::Io(e) => write!(f, "I/O 오류: {e}"),
            Self::Json(e) => write!(f, "JSON 파싱 오류: {e}"),
            Self::InvalidProtocol { expected, actual } => {
                write!(
                    f,
                    "지원하지 않는 프로토콜 (기대: {expected}, 실제: {actual})"
                )
            }
            Self::HttpError { status, message } => {
                write!(f, "HTTP 오류 ({status}): {message}")
            }
            Self::AssetNotFound(name) => write!(f, "애셋을 찾을 수 없음: {name}"),
            Self::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for GatewayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network(e) => Some(e),
            Self::Io(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for GatewayError {
    fn from(e: reqwest::Error) -> Self {
        Self::Network(e)
    }
}

impl From<std::io::Error> for GatewayError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for GatewayError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

impl From<String> for GatewayError {
    fn from(msg: String) -> Self {
        Self::Custom(msg)
    }
}

impl From<&str> for GatewayError {
    fn from(msg: &str) -> Self {
        Self::Custom(msg.to_string())
    }
}
