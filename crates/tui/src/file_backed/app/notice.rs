//! Notice severity and exact local-input ownership, independent of displayed text.
use alan_agent_protocol::{UiNoticeKind, UiNoticeSnapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) struct Notice {
    pub kind: UiNoticeKind,
    pub submission: Option<String>,
    pub runtime_owned: bool,
    message: String,
}

impl Notice {
    pub fn runtime(snapshot: UiNoticeSnapshot) -> Option<Self> {
        if snapshot.kind == UiNoticeKind::None || snapshot.message.trim().is_empty() {
            return None;
        }
        Some(Self {
            kind: snapshot.kind,
            submission: None,
            runtime_owned: true,
            message: humanize_protocol_message(&snapshot.message),
        })
    }

    pub fn queue(message: String, id: &str, uncertain: bool) -> Self {
        Self {
            kind: if uncertain {
                UiNoticeKind::Warning
            } else {
                UiNoticeKind::None
            },
            submission: Some(id.into()),
            runtime_owned: false,
            message,
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            kind: UiNoticeKind::Warning,
            ..Self::from(message.into())
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            kind: UiNoticeKind::Error,
            ..Self::from(message.into())
        }
    }
}

impl From<String> for Notice {
    fn from(message: String) -> Self {
        Self {
            kind: UiNoticeKind::None,
            submission: None,
            runtime_owned: false,
            message,
        }
    }
}

impl From<&str> for Notice {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl std::ops::Deref for Notice {
    type Target = str;

    fn deref(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Notice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// Rewrite low-level protocol/provider errors into clear, actionable guidance.
pub(in crate::file_backed) fn humanize_protocol_message(raw: &str) -> String {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("stream_error:authentication")
        || (lower.contains("stream_error") && lower.contains("auth"))
        || lower.contains("invalid_api_key")
        || lower.contains("unauthorized (401)")
    {
        return "Authentication failed for the active connection. Run `/login` (or `alan login` / `alan connection login <profile>` / `alan connection set-secret <profile>`).".to_string();
    }
    if lower.contains("no callable connection is available")
        || lower.contains("no connection service profile selected")
        || lower.contains("no connection profile selected")
        || lower.contains("no confirmed callable binding")
        || lower.contains("missing connection authority")
        || lower.contains("connection catalog unavailable")
    {
        return "No active model connection is configured. Run `/login` (or `alan login` / `alan connection add openai_responses --profile main --default`).".to_string();
    }
    if lower.contains("stream_error:rate_limit") {
        return "Provider rate limit reached. Wait a moment and retry, or switch models with `/model`.".to_string();
    }
    if let Some(rest) = trimmed.strip_prefix("stream_error:") {
        return format!("Provider stream error: {}", rest.trim());
    }
    trimmed.to_string()
}
