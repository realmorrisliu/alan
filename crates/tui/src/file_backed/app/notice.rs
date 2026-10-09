//! Notice severity and exact local-input ownership, independent of displayed text.
use alan_agent_protocol::{UiNoticeKind, UiNoticeSnapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::file_backed) struct Notice {
    pub kind: UiNoticeKind,
    pub submission: Option<String>,
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
            message: snapshot.message,
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
