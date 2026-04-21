use serde::ser::{Serialize, SerializeStruct, Serializer};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("session expired")]
    SessionExpired,
    #[error("rate limited")]
    RateLimited,
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("decode error: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::SessionExpired => "session_expired",
            AppError::RateLimited => "rate_limited",
            AppError::Network(_) => "network",
            AppError::Decode(_) => "decode",
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_expired_serializes_with_kind_and_message() {
        let e = AppError::SessionExpired;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "session_expired");
        assert!(v["message"].as_str().unwrap().contains("session"));
    }

    #[test]
    fn rate_limited_serializes_kind() {
        let e = AppError::RateLimited;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "rate_limited");
    }

    #[test]
    fn decode_error_serializes_with_source_message() {
        let inner = serde_json::from_str::<serde_json::Value>("not-json").unwrap_err();
        let e = AppError::Decode(inner);
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "decode");
        assert!(v["message"].as_str().unwrap().contains("decode error"));
    }
}
