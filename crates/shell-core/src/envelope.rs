use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

/// Version of the coarse-grained shell-core facade schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvelopeVersion {
    /// Incompatible schema family. Different major versions are rejected.
    pub major: u16,
    /// Backward-compatible schema revision within the current major version.
    pub minor: u16,
}

impl EnvelopeVersion {
    /// Current shell-core envelope schema version.
    pub const CURRENT: Self = Self { major: 1, minor: 0 };

    /// Returns whether this version can be handled by the current shell core.
    pub fn is_supported(self) -> bool {
        self.major == Self::CURRENT.major
    }
}

/// Stable shell-core error codes surfaced through envelopes and adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellCoreErrorCode {
    /// The request envelope uses an unsupported schema version.
    SchemaVersionMismatch,
    /// The request payload could not be decoded for the requested operation.
    InvalidPayload,
    /// The requested operation is not known by this shell-core facade.
    UnknownOperation,
}

impl ShellCoreErrorCode {
    /// Builds a structured error envelope with this code.
    pub fn envelope(self, message: impl Into<String>) -> ShellCoreErrorEnvelope {
        ShellCoreErrorEnvelope {
            code: self,
            message: message.into(),
            details: Map::new(),
        }
    }
}

/// Structured shell-core error payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellCoreErrorEnvelope {
    /// Stable error code for programmatic clients.
    pub code: ShellCoreErrorCode,
    /// Human-readable diagnostic text.
    pub message: String,
    /// Structured machine-readable details.
    #[serde(default)]
    pub details: Map<String, Value>,
}

impl ShellCoreErrorEnvelope {
    /// Adds a structured detail value to the error envelope.
    pub fn with_detail(mut self, key: impl Into<String>, value: Value) -> Self {
        self.details.insert(key.into(), value);
        self
    }
}

/// Versioned shell-core request envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellCoreRequestEnvelope {
    /// Request schema version.
    pub schema_version: EnvelopeVersion,
    /// Stable request id copied into the response envelope.
    pub id: Uuid,
    /// Coarse-grained operation name.
    pub operation: String,
    /// JSON payload for the requested operation.
    pub payload: Value,
}

impl ShellCoreRequestEnvelope {
    /// Creates a new request envelope using the current shell-core schema.
    pub fn new(operation: impl Into<String>, payload: Value) -> Self {
        Self {
            schema_version: EnvelopeVersion::CURRENT,
            id: Uuid::new_v4(),
            operation: operation.into(),
            payload,
        }
    }

    /// Validates that this request envelope uses a supported schema family.
    pub fn ensure_supported(&self) -> Result<(), ShellCoreErrorEnvelope> {
        if self.schema_version.is_supported() {
            return Ok(());
        }

        Err(ShellCoreErrorCode::SchemaVersionMismatch
            .envelope("unsupported shell-core schema version")
            .with_detail(
                "supported",
                serde_json::to_value(EnvelopeVersion::CURRENT)
                    .expect("EnvelopeVersion always serializes"),
            )
            .with_detail(
                "received",
                serde_json::to_value(self.schema_version)
                    .expect("EnvelopeVersion always serializes"),
            ))
    }
}

/// Versioned shell-core response envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShellCoreResponseEnvelope {
    /// Response schema version.
    pub schema_version: EnvelopeVersion,
    /// Request id copied from the request envelope.
    pub request_id: Uuid,
    /// Successful response payload.
    pub payload: Option<Value>,
    /// Structured error payload.
    pub error: Option<ShellCoreErrorEnvelope>,
}

impl ShellCoreResponseEnvelope {
    /// Creates a successful response envelope.
    pub fn success(request_id: Uuid, payload: Value) -> Self {
        Self {
            schema_version: EnvelopeVersion::CURRENT,
            request_id,
            payload: Some(payload),
            error: None,
        }
    }

    /// Creates an error response envelope.
    pub fn error(request_id: Uuid, error: ShellCoreErrorEnvelope) -> Self {
        Self {
            schema_version: EnvelopeVersion::CURRENT,
            request_id,
            payload: None,
            error: Some(error),
        }
    }
}
