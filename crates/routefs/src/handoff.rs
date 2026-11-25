//! Typed cross-workstream handoff message contracts and audit records for `alan-routefs`.

use alan_ap::ErrorCode;
use serde::{Deserialize, Serialize};

use crate::{DEAD_LETTER_PORT, ROOT_PORT, RuleSpec};

/// Current wire version for [`RouteHandoffMessage`].
pub const ROUTE_HANDOFF_VERSION: u16 = 1;
/// Maximum allowed cross-workstream routing hops (`hop_count <= 2`).
pub const MAX_HANDOFF_HOPS: u8 = 2;
/// Audit rule name recorded when a handoff exceeds [`MAX_HANDOFF_HOPS`].
pub const LOOP_GUARD_RULE_NAME: &str = "loop-guard";

/// Typed cross-workstream handoff kinds written to `/mnt/route/send`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteHandoffKind {
    /// Focused stream asks Root to reroute a misdirected prompt to another stream.
    RerouteRequest,
    /// Stream reports a dependency requiring work from another stream via Root.
    CrossStreamDependency,
    /// Stream returns a completed or failed handoff result back through Root.
    HandoffResult,
}

impl RouteHandoffKind {
    /// All supported handoff kinds in deterministic order.
    pub const ALL: [Self; 3] = [
        Self::RerouteRequest,
        Self::CrossStreamDependency,
        Self::HandoffResult,
    ];

    /// Canonical snake_case wire name used by `RuleSpec::match_type`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RerouteRequest => "reroute_request",
            Self::CrossStreamDependency => "cross_stream_dependency",
            Self::HandoffResult => "handoff_result",
        }
    }

    /// Default inspectable rule file name under `/mnt/route/rules/`.
    pub const fn default_rule_name(self) -> &'static str {
        match self {
            Self::RerouteRequest => "50-workstream-reroute-request",
            Self::CrossStreamDependency => "50-workstream-cross-stream-dependency",
            Self::HandoffResult => "50-workstream-handoff-result",
        }
    }

    /// Human-readable audit reason for the default Root routing rule.
    pub const fn default_rule_reason(self) -> &'static str {
        match self {
            Self::RerouteRequest => "route focused stream reroute_request to Root coordinator",
            Self::CrossStreamDependency => {
                "route cross_stream_dependency handoff to Root coordinator"
            }
            Self::HandoffResult => "route child handoff_result back to Root coordinator",
        }
    }
}

/// Typed cross-workstream handoff payload written to `/mnt/route/send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RouteHandoffMessage {
    pub version: u16,
    pub kind: RouteHandoffKind,
    pub source_stream: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggested_target: Option<String>,
    pub hop_count: u8,
    pub correlation_id: String,
    pub payload: String,
}

impl RouteHandoffMessage {
    /// Construct a `hop_count = 0` handoff message.
    pub fn new(
        kind: RouteHandoffKind,
        source_stream: impl Into<String>,
        correlation_id: impl Into<String>,
        payload: impl Into<String>,
    ) -> Self {
        Self {
            version: ROUTE_HANDOFF_VERSION,
            kind,
            source_stream: source_stream.into(),
            suggested_target: None,
            hop_count: 0,
            correlation_id: correlation_id.into(),
            payload: payload.into(),
        }
    }

    /// Attach a suggested destination workstream name (advisory only; Root owns dispatch).
    pub fn with_suggested_target(mut self, target: impl Into<String>) -> Self {
        self.suggested_target = Some(target.into());
        self
    }

    /// Set the explicit hop count on this handoff message.
    pub fn with_hop_count(mut self, hop_count: u8) -> Self {
        self.hop_count = hop_count;
        self
    }

    /// Return a copy with `hop_count` incremented by 1 (saturating at `u8::MAX`).
    pub fn next_hop(mut self) -> Self {
        self.hop_count = self.hop_count.saturating_add(1);
        self
    }

    /// Returns `true` when `hop_count > MAX_HANDOFF_HOPS` (`> 2`).
    pub const fn exceeds_max_hops(&self) -> bool {
        self.hop_count > MAX_HANDOFF_HOPS
    }

    /// Validate structural invariants of this handoff message.
    ///
    /// Note that `hop_count > MAX_HANDOFF_HOPS` is structurally valid so `routefs`
    /// can accept the write, route it to `dead-letter`, and log the escalation.
    pub fn validate(&self) -> Result<(), ErrorCode> {
        if self.version != ROUTE_HANDOFF_VERSION
            || !valid_stream_token(&self.source_stream)
            || !valid_correlation_id(&self.correlation_id)
        {
            return Err(ErrorCode::BadRequest);
        }
        if let Some(target) = self.suggested_target.as_deref()
            && !valid_stream_token(target)
        {
            return Err(ErrorCode::BadRequest);
        }
        Ok(())
    }
}

/// Auditable routing decision record written to `ports/<port>` and `log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutedRecord {
    pub version: u16,
    pub port: String,
    pub rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub message: serde_json::Value,
}

impl RoutedRecord {
    /// Parse `message` as a [`RouteHandoffMessage`] when the routed payload is a typed handoff.
    pub fn handoff_message(&self) -> Option<RouteHandoffMessage> {
        serde_json::from_value::<RouteHandoffMessage>(self.message.clone())
            .ok()
            .filter(|msg| msg.validate().is_ok())
    }

    /// Returns `true` when this record was delivered to the `dead-letter` port.
    pub fn is_dead_letter(&self) -> bool {
        self.port == DEAD_LETTER_PORT
    }

    /// Returns `true` when this record was dead-lettered by the hop-count loop guard.
    pub fn is_loop_escalation(&self) -> bool {
        self.port == DEAD_LETTER_PORT && self.rule == LOOP_GUARD_RULE_NAME
    }
}

/// Default inspectable rules routing workstream handoff kinds to `/mnt/route/ports/root`.
pub fn workstream_handoff_rules() -> Vec<(&'static str, RuleSpec)> {
    RouteHandoffKind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind.default_rule_name(),
                RuleSpec::for_type(kind.as_str(), ROOT_PORT)
                    .with_reason(kind.default_rule_reason()),
            )
        })
        .collect()
}

pub(crate) fn valid_stream_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !value.contains('/') && !value.contains('\n') && !value.contains('\r')
}

pub(crate) fn valid_correlation_id(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !value.contains('\n') && !value.contains('\r')
}

pub(crate) fn parse_optional_hop_count(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<u8>, ErrorCode> {
    let Some(raw) = object.get("hop_count") else {
        return Ok(None);
    };
    let hops = raw.as_u64().ok_or(ErrorCode::BadRequest)?;
    Ok(Some(u8::try_from(hops).unwrap_or(u8::MAX)))
}
