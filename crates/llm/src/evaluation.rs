//! Provider-neutral finite-choice advice. These types carry no execution authority.

use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::TokenUsage;

/// One Process-resolved candidate offered to an evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationCandidate {
    pub id: String,
    pub description: String,
}

/// Bounded input for the finite-choice operation; never a generation request.
#[derive(Debug, Clone)]
pub struct ChoiceEvaluationRequest {
    pub input: String,
    pub candidates: Vec<EvaluationCandidate>,
}

impl ChoiceEvaluationRequest {
    /// Validate at the adapter boundary without rewriting input or candidate identity.
    pub fn validate(&self) -> Result<()> {
        if self.candidates.is_empty() || self.candidates.len() > 64 {
            bail!("evaluation requires 1 to 64 candidates");
        }
        let mut ids = HashSet::new();
        let mut bytes = self.input.len();
        for candidate in &self.candidates {
            if candidate.id.is_empty()
                || candidate.id.len() > 128
                || !candidate
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                || !ids.insert(&candidate.id)
            {
                bail!("invalid or duplicate evaluation candidate ID");
            }
            bytes = bytes
                .saturating_add(candidate.id.len())
                .saturating_add(candidate.description.len());
        }
        if bytes > 1 << 20 {
            bail!("evaluation content exceeds 1 MiB");
        }
        Ok(())
    }

    /// A provider cannot expand the caller's finite candidate set.
    pub fn validate_selection(&self, selection: &EvaluationSelection) -> Result<()> {
        self.validate()?;
        if let EvaluationSelection::Selected(id) = selection
            && !self.candidates.iter().any(|candidate| candidate.id == *id)
        {
            bail!("evaluation selected a candidate outside the request");
        }
        Ok(())
    }
}

/// Typed advice, never assistant text or an executable command body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvaluationSelection {
    Selected(String),
    NoMatch,
}

/// A validated provider result. Missing usage is unknown, not zero.
#[derive(Debug, Clone)]
pub struct ChoiceEvaluationResponse {
    pub selection: EvaluationSelection,
    pub usage: Option<TokenUsage>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finite_choice_keeps_original_input_and_rejects_expanded_authority() {
        let mut request = ChoiceEvaluationRequest {
            input: "  quoted $(touch marker)\n".into(),
            candidates: vec![EvaluationCandidate {
                id: "agent".into(),
                description: "Agent advice".into(),
            }],
        };
        let original = request.input.clone();
        request.validate().unwrap();
        request
            .validate_selection(&EvaluationSelection::Selected("agent".into()))
            .unwrap();
        request
            .validate_selection(&EvaluationSelection::NoMatch)
            .unwrap();
        assert_eq!(request.input, original);
        assert!(
            request
                .validate_selection(&EvaluationSelection::Selected("command".into()))
                .is_err()
        );
        request.candidates.push(request.candidates[0].clone());
        assert!(request.validate().is_err());
        request.candidates.pop();
        request.candidates[0].id = "../command".into();
        assert!(request.validate().is_err());
        request.candidates[0].id = "agent".into();
        request.input = "x".repeat((1 << 20) + 1);
        assert!(request.validate().is_err());
    }
}
