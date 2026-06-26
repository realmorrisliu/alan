//! Content symbols — the alphabet of the tape.
//!
//! `ContentPart` is the basic unit of content in the alan protocol.
//! It lives in `alan-agent-protocol` so that `Op` can reference it directly,
//! and `alan-agent-engine`'s tape re-exports it.

use serde::{Deserialize, Serialize};

/// A content symbol — the basic unit of the protocol alphabet.
/// These are "nouns": passive carriers of information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// Standard text content.
    Text { text: String },

    /// Thinking chain / reasoning process.
    Thinking {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },

    /// Redacted thinking block returned by Anthropic APIs.
    RedactedThinking { data: String },

    /// Multimodal attachment (images, files, audio, etc.)
    Attachment {
        hash: String,
        mime_type: String,
        #[serde(default)]
        metadata: serde_json::Value,
    },

    /// Structured data — native expression, no longer degraded to JSON strings.
    Structured { data: serde_json::Value },
}

impl ContentPart {
    /// Create a text content part.
    pub fn text(s: impl Into<String>) -> Self {
        ContentPart::Text { text: s.into() }
    }

    /// Create a thinking content part.
    pub fn thinking(s: impl Into<String>) -> Self {
        ContentPart::Thinking {
            text: s.into(),
            signature: None,
        }
    }

    /// Create a thinking content part with signature metadata.
    pub fn thinking_with_signature(text: impl Into<String>, signature: impl Into<String>) -> Self {
        ContentPart::Thinking {
            text: text.into(),
            signature: Some(signature.into()),
        }
    }

    /// Create a redacted thinking content part.
    pub fn redacted_thinking(data: impl Into<String>) -> Self {
        ContentPart::RedactedThinking { data: data.into() }
    }

    /// Create a structured content part.
    pub fn structured(data: serde_json::Value) -> Self {
        ContentPart::Structured { data }
    }

    /// Extract the text content, if this is a Text or Thinking part.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            ContentPart::Text { text } | ContentPart::Thinking { text, .. } => Some(text),
            _ => None,
        }
    }

    /// Convert any content part to a text representation.
    /// Text/Thinking return their content directly.
    /// Structured serializes to JSON string.
    /// Attachment returns a placeholder.
    pub fn to_text_lossy(&self) -> String {
        match self {
            ContentPart::Text { text } | ContentPart::Thinking { text, .. } => text.clone(),
            ContentPart::RedactedThinking { .. } => "[redacted_thinking]".to_string(),
            ContentPart::Structured { data } => {
                serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string())
            }
            ContentPart::Attachment {
                hash, mime_type, ..
            } => {
                format!("[attachment: {} ({})]", hash, mime_type)
            }
        }
    }
}

/// Helper: extract concatenated text from a slice of ContentParts (non-thinking only).
pub fn parts_to_text(parts: &[ContentPart]) -> String {
    parts
        .iter()
        .filter_map(|p| match p {
            ContentPart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_part_serde_roundtrip() {
        let parts = vec![
            ContentPart::text("hello"),
            ContentPart::thinking("hmm"),
            ContentPart::structured(serde_json::json!({"key": "value"})),
        ];
        let json = serde_json::to_string(&parts).unwrap();
        let deserialized: Vec<ContentPart> = serde_json::from_str(&json).unwrap();
        assert_eq!(parts, deserialized);
    }

    #[test]
    fn test_content_part_constructors() {
        let text = ContentPart::text("hello");
        assert_eq!(text.as_text(), Some("hello"));

        let thinking = ContentPart::thinking("reasoning...");
        assert_eq!(thinking.as_text(), Some("reasoning..."));

        let structured = ContentPart::structured(serde_json::json!({"key": "value"}));
        assert!(structured.as_text().is_none());
    }

    #[test]
    fn test_parts_to_text() {
        let parts = vec![
            ContentPart::text("hello "),
            ContentPart::thinking("internal"),
            ContentPart::text("world"),
        ];
        assert_eq!(parts_to_text(&parts), "hello world");
    }
}
