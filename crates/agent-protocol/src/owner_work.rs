//! Explicit, bounded source-ownership work; ordinary text never selects this program.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Explicit Machine control envelope retaining the ordinary submission UUID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerWorkControl {
    pub id: uuid::Uuid,
    pub request: OwnerWorkRequest,
}

impl OwnerWorkControl {
    /// Encode the control; clients must not retry an uncertain write with a new ID.
    pub fn encode(&self) -> Result<String, serde_json::Error> {
        self.request.validate().map_err(serde::ser::Error::custom)?;
        let control = format!("owner-work-v1 {}", serde_json::to_string(self)?);
        if control.len() > 64 * 1024 {
            return Err(serde::ser::Error::custom(
                "owner work control exceeds document limit",
            ));
        }
        Ok(control)
    }

    /// Admit typed work through the existing ForceAgent FollowUp queue.
    pub fn into_submission(self) -> Result<crate::Submission, serde_json::Error> {
        self.encode()?;
        Ok(crate::Submission {
            id: self.id.to_string(),
            intent: crate::InputIntent::ForceAgent,
            op: crate::Op::Input {
                parts: vec![crate::ContentPart::structured(serde_json::json!({
                    "owner_work_v1": self.request,
                }))],
                mode: crate::InputMode::FollowUp,
            },
        })
    }
}

/// One source range resolved through the submitting Agent's namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSourceRange {
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
}

/// An exposed owner candidate, with explicit source evidence rather than Host paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerCandidate {
    pub id: String,
    pub sources: Vec<OwnerSourceRange>,
}

/// The first mixed-Machine program: select an owner from read-only source evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerWorkRequest {
    pub version: u8,
    pub question: String,
    pub evaluator_profile: String,
    pub candidates: Vec<OwnerCandidate>,
}

impl OwnerWorkRequest {
    /// Validate controls before admission; source visibility is resolved by the Machine.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 1 {
            return Err("unsupported owner work version");
        }
        if self.question.trim().is_empty() || self.question.len() > 8192 {
            return Err("invalid owner work question");
        }
        if !component(&self.evaluator_profile) {
            return Err("invalid evaluator profile");
        }
        if !(1..=16).contains(&self.candidates.len()) {
            return Err("owner work requires 1 to 16 candidates");
        }
        let mut ids = HashSet::new();
        for candidate in &self.candidates {
            if !component(&candidate.id) || !ids.insert(&candidate.id) {
                return Err("invalid or duplicate owner candidate");
            }
            if !(1..=8).contains(&candidate.sources.len()) {
                return Err("owner candidate requires 1 to 8 source ranges");
            }
            if serde_json::to_vec(&candidate.sources)
                .map_err(|_| "invalid sources")?
                .len()
                > 4096
            {
                return Err("owner citations exceed result budget");
            }
            for source in &candidate.sources {
                if source.path.len() > 4096
                    || !source.path.starts_with("/mnt/")
                    || source.path.chars().any(char::is_control)
                    || source.path[1..]
                        .split('/')
                        .any(|s| matches!(s, "" | "." | ".."))
                    || source.start_line == 0
                    || source.end_line < source.start_line
                    || source.end_line - source.start_line >= 1000
                {
                    return Err("invalid namespace source range");
                }
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| "invalid owner work")?
            .len()
            > 64 * 1024
        {
            return Err("owner work exceeds document limit");
        }
        Ok(())
    }
}

fn component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !matches!(value, "." | "..")
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_authority_descriptors_and_finite_candidates() {
        let mut request = OwnerWorkRequest {
            version: 1,
            question: "Which crate defines HostDirFs?".into(),
            evaluator_profile: "typesafe-main".into(),
            candidates: vec![OwnerCandidate {
                id: "hostfs".into(),
                sources: vec![OwnerSourceRange {
                    path: "/mnt/project/crates/hostfs/src/lib.rs".into(),
                    start_line: 40,
                    end_line: 50,
                }],
            }],
        };
        assert!(request.validate().is_ok());
        for path in [
            "/Users/secret",
            "/mnt/project/../secret",
            "/mnt//secret",
            "/mnt/x\n",
        ] {
            request.candidates[0].sources[0].path = path.into();
            assert!(request.validate().is_err(), "{path}");
        }
        request.candidates[0].sources[0].path = "/mnt/project/file.rs".into();
        request.candidates.push(request.candidates[0].clone());
        assert!(request.validate().is_err());
        let mut value = serde_json::to_value(request).unwrap();
        value["cost_microusd"] = serde_json::json!(0);
        assert!(serde_json::from_value::<OwnerWorkRequest>(value).is_err());
    }
}
