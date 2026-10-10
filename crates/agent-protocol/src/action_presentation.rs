use serde::{Deserialize, Serialize};

/// Runtime-owned correlation for grouping successful native read-only Actions.
/// This is presentation evidence, never execution authority or a Tool payload claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionReadOnlyContext {
    /// Concrete Agent Process view that owns this Action.
    pub owner: String,
    /// Accepted input identity from the advancing Agent Machine.
    pub submission: String,
    /// Opaque identity of the unchanged live authority and selected directory.
    pub authority: String,
}

impl ActionReadOnlyContext {
    /// Reject incomplete, oversized and non-concrete metadata with standalone fallback.
    pub fn is_valid(&self) -> bool {
        self.owner
            .strip_prefix("/agent/")
            .and_then(|pid| pid.parse::<u64>().ok())
            .is_some_and(|pid| pid > 0 && self.owner == format!("/agent/{pid}"))
            && !self.submission.is_empty()
            && self.submission.len() <= 256
            && !self.submission.chars().any(char::is_control)
            && !self.authority.is_empty()
            && self.authority.len() <= 4096
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_context_requires_bounded_concrete_correlation() {
        let valid = ActionReadOnlyContext {
            owner: "/agent/7".into(),
            submission: "q1".into(),
            authority: "opaque".into(),
        };
        assert!(valid.is_valid());
        assert_eq!(
            serde_json::from_str::<ActionReadOnlyContext>(&serde_json::to_string(&valid).unwrap())
                .unwrap(),
            valid
        );
        for owner in [
            "/agent/root",
            "/agent/0",
            "/agent/07",
            "/agent/+7",
            "/agent/7/child",
            "",
        ] {
            assert!(
                !ActionReadOnlyContext {
                    owner: owner.into(),
                    ..valid.clone()
                }
                .is_valid()
            );
        }
        for submission in [String::new(), "q\n1".into(), "q".repeat(257)] {
            assert!(
                !ActionReadOnlyContext {
                    submission,
                    ..valid.clone()
                }
                .is_valid()
            );
        }
        for authority in [String::new(), "x".repeat(4097)] {
            assert!(
                !ActionReadOnlyContext {
                    authority,
                    ..valid.clone()
                }
                .is_valid()
            );
        }
    }
}
