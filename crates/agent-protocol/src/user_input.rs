//! Versioned input payloads; AgentFS continues to own the outer length frame.
use serde::{Deserialize, Serialize};

use crate::InputMode;

/// One-shot routing intent, independent from input scheduling mode.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InputIntent {
    /// Ordinary Agent input; automatic classification is not enabled.
    #[default]
    Agent,
    /// Explicit `:` input, always interpreted by the Agent.
    ForceAgent,
    /// Explicit `!` input, admitted through governed command execution.
    Command,
}

/// Consume at most one prefix, preserving every byte of the remaining body.
pub fn parse_input_prefix(input: &str) -> (InputIntent, &str) {
    if let Some(body) = input.strip_prefix('!') {
        (InputIntent::Command, body)
    } else if let Some(body) = input.strip_prefix(':') {
        (InputIntent::ForceAgent, body)
    } else {
        (InputIntent::Agent, input)
    }
}

/// Payload identity and intent for one committed AgentFS input frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UserInputRecord {
    /// Supported wire schema version.
    pub version: u8,
    /// UUID retained from admission through the correlated completion.
    pub submission_id: String,
    /// Interpretation of the body, separate from when it is scheduled.
    pub intent: InputIntent,
    /// Existing runtime scheduling contract.
    pub mode: InputMode,
    /// Exact body after consuming at most one prefix.
    pub body: String,
}

const MAGIC: &[u8] = b"alan-input-v1\n";

impl UserInputRecord {
    /// Create a record with a new submission identity.
    pub fn new(intent: InputIntent, mode: InputMode, body: impl Into<String>) -> Self {
        Self {
            version: 1,
            submission_id: uuid::Uuid::new_v4().to_string(),
            intent,
            mode,
            body: body.into(),
        }
    }

    /// Validate a record before encoding or admission.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 {
            return Err("unsupported user input record version");
        }
        if uuid::Uuid::parse_str(&self.submission_id).is_err() {
            return Err("user input submission_id must be a UUID");
        }
        if self.body.trim().is_empty() {
            return Err("user input body is empty");
        }
        Ok(())
    }

    /// Encode a validated record without AgentFS's outer length frame.
    pub fn encode_payload(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.validate().map_err(serde::ser::Error::custom)?;
        let mut payload = MAGIC.to_vec();
        payload.extend(serde_json::to_vec(self)?);
        Ok(payload)
    }

    /// Decode and validate a record; `None` denotes legacy plain-text input.
    /// A malformed or unsupported record never falls back to Agent prose.
    pub fn decode_payload(payload: &[u8]) -> Result<Option<Self>, serde_json::Error> {
        let Some(json) = payload.strip_prefix(MAGIC) else {
            return if payload.starts_with(b"alan-input-") {
                Err(serde::de::Error::custom(
                    "unsupported user input record framing",
                ))
            } else {
                Ok(None)
            };
        };
        let record: Self = serde_json::from_slice(json)?;
        record.validate().map_err(serde::de::Error::custom)?;
        Ok(Some(record))
    }
}
