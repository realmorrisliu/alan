//! Cross-workstream handoff coordination over `/mnt/route` (`alan-routefs`).
//!
//! Communicates exclusively via the aP protocol (`InProcessTransport`) against
//! `/mnt/route/send`, `/mnt/route/rules/*`, `/mnt/route/ports/root`,
//! `/mnt/route/ports/dead-letter`, and `/mnt/route/log`.
//!
//! Enforces:
//! - Typed [`RouteHandoffMessage`] contracts (`reroute_request`,
//!   `cross_stream_dependency`, `handoff_result`).
//! - Loop protection (`MAX_HANDOFF_HOPS = 2`, escalating exceeded hops via
//!   `dead-letter` to Root).
//! - Zero authority widening or implicit mount inheritance across streams.

mod ap_client;
mod coordinator;
#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize};

pub use ap_client::{
    RouteStreamCursor, ensure_workstream_route_rules, ensure_workstream_route_rules_at,
    read_route_audit_log, read_route_audit_log_at, send_handoff_message, send_handoff_message_at,
};
pub use coordinator::{
    HandoffDispatchPlan, HandoffDisposition, RoutingReceipt, RoutingReceiptKind,
    WorkstreamAuthorityProfile, WorkstreamHandoffCoordinator, validate_isolated_handoff_mounts,
};

/// Canonical mount path for `alan-routefs`.
pub const ROUTE_MOUNT_PATH: &str = "/mnt/route";
/// Canonical port receiving Root-bound cross-stream handoffs.
pub const ROUTE_ROOT_PORT: &str = "root";
/// Canonical port receiving unmatched or loop-bounded handoffs.
pub const ROUTE_DEAD_LETTER_PORT: &str = "dead-letter";
/// Rule name recorded by `routefs` when loop protection dead-letters a message.
pub const ROUTE_LOOP_GUARD_RULE: &str = "loop-guard";
/// Wire version for [`RouteHandoffMessage`] and [`WorkstreamRouteRuleSpec`].
pub const ROUTE_HANDOFF_VERSION: u16 = 1;
/// Maximum allowed cross-stream handoff hops (`hop_count <= 2`).
pub const MAX_HANDOFF_HOPS: u8 = 2;

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

    /// Canonical wire type string matched by `/mnt/route/rules/*`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RerouteRequest => "reroute_request",
            Self::CrossStreamDependency => "cross_stream_dependency",
            Self::HandoffResult => "handoff_result",
        }
    }

    /// Inspectable rule file name under `/mnt/route/rules/`.
    pub const fn default_rule_name(self) -> &'static str {
        match self {
            Self::RerouteRequest => "50-workstream-reroute-request",
            Self::CrossStreamDependency => "50-workstream-cross-stream-dependency",
            Self::HandoffResult => "50-workstream-handoff-result",
        }
    }

    /// Audit reason recorded in the rule file and `/mnt/route/log`.
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

/// Typed cross-stream handoff message written to `/mnt/route/send`.
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

    /// Set an advisory target workstream name (Root validates & owns actual dispatch).
    pub fn with_suggested_target(mut self, target: impl Into<String>) -> Self {
        self.suggested_target = Some(target.into());
        self
    }

    /// Set an explicit hop count.
    pub fn with_hop_count(mut self, hop_count: u8) -> Self {
        self.hop_count = hop_count;
        self
    }

    /// Return a copy with `hop_count` incremented by 1.
    pub fn next_hop(mut self) -> Self {
        self.hop_count = self.hop_count.saturating_add(1);
        self
    }

    /// Returns `true` when `hop_count > MAX_HANDOFF_HOPS`.
    pub const fn exceeds_max_hops(&self) -> bool {
        self.hop_count > MAX_HANDOFF_HOPS
    }

    /// Validate structural invariants before writing to `/mnt/route/send`.
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.version == ROUTE_HANDOFF_VERSION,
            "unsupported RouteHandoffMessage version {}",
            self.version
        );
        anyhow::ensure!(
            valid_stream_token(&self.source_stream),
            "invalid handoff source_stream `{}`",
            self.source_stream
        );
        anyhow::ensure!(
            !self.correlation_id.trim().is_empty()
                && !self.correlation_id.contains('\n')
                && !self.correlation_id.contains('\r'),
            "invalid handoff correlation_id"
        );
        if let Some(target) = self.suggested_target.as_deref() {
            anyhow::ensure!(
                valid_stream_token(target),
                "invalid handoff suggested_target `{target}`"
            );
        }
        Ok(())
    }
}

/// Plain JSON rule definition written under `/mnt/route/rules/<name>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkstreamRouteRuleSpec {
    pub version: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_contains: Option<String>,
    pub port: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Default inspectable rules routing workstream handoff types to `/mnt/route/ports/root`.
pub fn default_workstream_route_rules() -> Vec<(&'static str, WorkstreamRouteRuleSpec)> {
    RouteHandoffKind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind.default_rule_name(),
                WorkstreamRouteRuleSpec {
                    version: ROUTE_HANDOFF_VERSION,
                    match_type: Some(kind.as_str().to_string()),
                    content_contains: None,
                    port: ROUTE_ROOT_PORT.to_string(),
                    reason: Some(kind.default_rule_reason().to_string()),
                },
            )
        })
        .collect()
}

/// Auditable routing decision record read from `/mnt/route/ports/*` or `/mnt/route/log`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutedRouteRecord {
    pub version: u16,
    pub port: String,
    pub rule: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub message: serde_json::Value,
}

impl RoutedRouteRecord {
    /// Extract the typed [`RouteHandoffMessage`] when this record carries a valid handoff.
    pub fn handoff_message(&self) -> Option<RouteHandoffMessage> {
        serde_json::from_value::<RouteHandoffMessage>(self.message.clone())
            .ok()
            .filter(|msg| msg.validate().is_ok())
    }

    /// Returns `true` when delivered to `dead-letter`.
    pub fn is_dead_letter(&self) -> bool {
        self.port == ROUTE_DEAD_LETTER_PORT
    }

    /// Returns `true` when dead-lettered specifically by the hop-count loop guard.
    pub fn is_loop_escalation(&self) -> bool {
        self.port == ROUTE_DEAD_LETTER_PORT && self.rule == ROUTE_LOOP_GUARD_RULE
    }
}

fn valid_stream_token(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && !value.contains('/') && !value.contains('\n') && !value.contains('\r')
}
