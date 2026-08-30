use serde::{Serialize, Serializer};

/// Stable IPC error codes. New variants should be added as later phases need them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    Config,
    Io,
    InvalidInput,
    Conflict,
    Db,
    Provider,
    Internal,
}

/// Structured payload returned to the frontend on command failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ErrorPayload {
    pub code: ErrorCode,
    pub message: String,
}

/// Application error type used by IPC commands and core bootstrap logic.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Config { message: String },
    #[error("{message}")]
    Io { message: String },
    #[error("{message}")]
    InvalidInput { message: String },
    #[allow(dead_code)]
    #[error("{message}")]
    Conflict { message: String },
    #[error("{message}")]
    Db { message: String },
    /// Failure originating from an external AI provider (network, malformed response).
    #[error("{message}")]
    Provider { message: String },
    /// Catch-all for unexpected failures. Used by later phases.
    #[allow(dead_code)]
    #[error("{message}")]
    Internal { message: String },
}

impl AppError {
    pub fn config(message: impl Into<String>) -> Self {
        Self::Config {
            message: message.into(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self::Io {
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput {
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict {
            message: message.into(),
        }
    }

    pub fn db(message: impl Into<String>) -> Self {
        Self::Db {
            message: message.into(),
        }
    }

    pub fn provider(message: impl Into<String>) -> Self {
        Self::Provider {
            message: message.into(),
        }
    }

    #[allow(dead_code)]
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            Self::Config { .. } => ErrorCode::Config,
            Self::Io { .. } => ErrorCode::Io,
            Self::InvalidInput { .. } => ErrorCode::InvalidInput,
            Self::Conflict { .. } => ErrorCode::Conflict,
            Self::Db { .. } => ErrorCode::Db,
            Self::Provider { .. } => ErrorCode::Provider,
            Self::Internal { .. } => ErrorCode::Internal,
        }
    }

    pub fn payload(&self) -> ErrorPayload {
        ErrorPayload {
            code: self.code(),
            message: self.to_string(),
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.payload().serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_structured_payload() {
        let error = AppError::invalid_input("name must not be empty");
        let json = serde_json::to_value(&error).expect("serialize");

        assert_eq!(json["code"], "INVALID_INPUT");
        assert_eq!(json["message"], "name must not be empty");
    }

    #[test]
    fn maps_variants_to_codes() {
        assert_eq!(AppError::config("bad").code(), ErrorCode::Config);
        assert_eq!(AppError::io("disk").code(), ErrorCode::Io);
        assert_eq!(AppError::internal("bug").code(), ErrorCode::Internal);
    }
}
