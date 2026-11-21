//! Gemini thought-signature carriage through the provider-neutral
//! `thinking_signature` string.
//!
//! Gemini attaches opaque `thoughtSignature`s to specific response parts (the
//! first function call of each step, or the final text part). Alan stores one
//! signature string per assistant message, so the per-part positions are
//! encoded into a prefixed JSON envelope. Strings without the prefix come from
//! other providers and are never forwarded to Gemini.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

const PREFIX: &str = "alan.gemini.sig.v1:";

/// Documented validator bypass for function calls that have no signature
/// (for example history produced by another model).
pub(super) const SKIP_SIGNATURE_VALIDATOR: &str = "skip_thought_signature_validator";

/// Signatures observed on one model turn, keyed by part role.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct TurnSignatures {
    /// Function-call ordinal within the turn -> signature.
    #[serde(default, rename = "c", skip_serializing_if = "BTreeMap::is_empty")]
    calls: BTreeMap<usize, String>,
    /// Signature carried on a text/thought part (replayed on the last text part).
    #[serde(default, rename = "t", skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

impl TurnSignatures {
    pub(super) fn record_call(&mut self, ordinal: usize, signature: Option<&str>) {
        if let Some(signature) = signature.filter(|value| !value.is_empty()) {
            self.calls.insert(ordinal, signature.to_string());
        }
    }

    pub(super) fn record_text(&mut self, signature: Option<&str>) {
        if let Some(signature) = signature.filter(|value| !value.is_empty()) {
            self.text = Some(signature.to_string());
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.text.is_none()
    }

    /// Encode for `thinking_signature`; `None` when nothing was observed.
    pub(super) fn encode(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        serde_json::to_string(self)
            .ok()
            .map(|json| format!("{PREFIX}{json}"))
    }

    /// Decode a stored signature; foreign or malformed values decode to empty.
    pub(super) fn decode(stored: Option<&str>) -> Self {
        stored
            .and_then(|value| value.strip_prefix(PREFIX))
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default()
    }

    pub(super) fn call(&self, ordinal: usize) -> Option<&str> {
        self.calls.get(&ordinal).map(String::as_str)
    }

    pub(super) fn has_any_call_signature(&self) -> bool {
        !self.calls.is_empty()
    }

    pub(super) fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

/// Whether the model validates function-call signatures (Gemini 3.x / 4.x+).
pub(super) fn model_requires_call_signatures(model: &str) -> bool {
    crate::gemini_catalog::model_requires_call_signatures(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_positions_and_rejects_foreign_values() {
        let mut signatures = TurnSignatures::default();
        signatures.record_call(0, Some("sig-a"));
        signatures.record_call(2, Some("sig-c"));
        signatures.record_call(1, Some(""));
        signatures.record_text(Some("sig-t"));
        let encoded = signatures.encode().unwrap();
        assert!(encoded.starts_with(PREFIX));

        let decoded = TurnSignatures::decode(Some(&encoded));
        assert_eq!(decoded, signatures);
        assert_eq!(decoded.call(0), Some("sig-a"));
        assert_eq!(decoded.call(1), None);
        assert_eq!(decoded.text(), Some("sig-t"));

        assert!(TurnSignatures::decode(Some("anthropic-signature")).is_empty());
        assert!(TurnSignatures::decode(Some("alan.gemini.sig.v1:{bad")).is_empty());
        assert_eq!(TurnSignatures::default().encode(), None);
    }

    #[test]
    fn signature_validation_applies_to_gemini_3_and_later() {
        assert!(model_requires_call_signatures("gemini-3-pro-preview"));
        assert!(model_requires_call_signatures(
            "gemini-3.1-flash-lite-preview"
        ));
        assert!(model_requires_call_signatures("gemini-4.0-pro-preview"));
        assert!(model_requires_call_signatures("gemini-10-pro"));
        assert!(!model_requires_call_signatures("gemini-2.5-pro"));
        assert!(!model_requires_call_signatures("gemini-1.5-pro"));
    }
}
