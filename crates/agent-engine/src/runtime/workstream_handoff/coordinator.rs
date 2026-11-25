//! Root-owned cross-stream handoff resolution, loop protection, and zero-authority-widening validation.

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use alan_agent_protocol::{
    ProcessNamespaceAccess, ProcessNamespaceMount, SpawnHostMount, SpawnMountAccess,
};
use alan_ap::InProcessTransport;
use anyhow::Result;

use crate::runtime::project_workstreams::{
    ProjectLedger, WorkstreamRecord, WorkstreamState, WorkstreamWriteMode,
};

use super::{
    MAX_HANDOFF_HOPS, ROUTE_DEAD_LETTER_PORT, ROUTE_MOUNT_PATH, ROUTE_ROOT_PORT, RouteHandoffKind,
    RouteHandoffMessage, RouteStreamCursor, RoutedRouteRecord, ensure_workstream_route_rules_at,
};

const MAX_TRACKED_CORRELATIONS: usize = 1024;

/// Classification for inline TUI routing/handoff receipts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingReceiptKind {
    Reroute,
    CrossStreamDependency,
    HandoffResult,
    LoopEscalation,
    TargetUnavailable,
}

/// Calm, inspectable inline receipt emitted for routing/handoff decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingReceipt {
    pub kind: RoutingReceiptKind,
    pub source_stream: String,
    pub target_stream: Option<String>,
    pub correlation_id: String,
    pub hop_count: u8,
    pub detail: Option<String>,
}

impl RoutingReceipt {
    /// Format the single-line calm receipt suitable for inline transcript rendering.
    pub fn format_inline(&self) -> String {
        let target = self.target_stream.as_deref().unwrap_or(ROUTE_ROOT_PORT);
        match self.kind {
            RoutingReceiptKind::Reroute => {
                format!("↳ rerouted from [{}] → [{target}]", self.source_stream)
            }
            RoutingReceiptKind::CrossStreamDependency => {
                format!(
                    "↳ cross-stream handoff [{}] → [{target}]",
                    self.source_stream
                )
            }
            RoutingReceiptKind::HandoffResult => {
                format!("↳ handoff result [{}] → [{target}]", self.source_stream)
            }
            RoutingReceiptKind::LoopEscalation => format!(
                "↳ handoff escalated [{}] → [{ROUTE_DEAD_LETTER_PORT}] (hop {} > max {MAX_HANDOFF_HOPS})",
                self.source_stream, self.hop_count
            ),
            RoutingReceiptKind::TargetUnavailable => {
                let reason = self
                    .detail
                    .as_deref()
                    .unwrap_or("target stream unavailable");
                format!(
                    "↳ handoff escalated [{}] → [{ROUTE_ROOT_PORT}] ({reason})",
                    self.source_stream
                )
            }
        }
    }
}

/// Validated target-owned execution authority for a handoff dispatch.
///
/// Built exclusively from the target workstream's own validated Host Mount grants
/// and live `task_budget`; never inherits grants or write scopes from the source stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkstreamAuthorityProfile {
    pub stream_name: String,
    pub read_only: bool,
    pub write_mode: WorkstreamWriteMode,
    pub host_mounts: Vec<SpawnHostMount>,
    pub budget_available: bool,
}

impl WorkstreamAuthorityProfile {
    /// Validate `target_mounts` strictly against `parent_mounts` and `read_only` posture.
    ///
    /// Guarantees zero authority widening and zero implicit mount inheritance from any
    /// source workstream.
    pub fn validated_against_parent(
        stream_name: impl Into<String>,
        read_only: bool,
        write_mode: WorkstreamWriteMode,
        target_mounts: &[SpawnHostMount],
        parent_mounts: &[ProcessNamespaceMount],
        budget_available: bool,
    ) -> std::result::Result<Self, String> {
        let stream_name = stream_name.into();
        if !budget_available {
            return Err(format!(
                "target workstream `{stream_name}` failed live task_budget validation"
            ));
        }
        let effective_read_only = read_only || write_mode == WorkstreamWriteMode::ReadOnly;
        let validated_mounts = validate_isolated_handoff_mounts(
            &stream_name,
            effective_read_only,
            target_mounts,
            parent_mounts,
        )?;
        Ok(Self {
            stream_name,
            read_only: effective_read_only,
            write_mode,
            host_mounts: validated_mounts,
            budget_available,
        })
    }
}

/// Validate that a target stream's `target_mounts` are equal-or-narrower subsets of
/// `parent_mounts` and never widen a `read_only` posture.
pub fn validate_isolated_handoff_mounts(
    stream_name: &str,
    read_only: bool,
    target_mounts: &[SpawnHostMount],
    parent_mounts: &[ProcessNamespaceMount],
) -> std::result::Result<Vec<SpawnHostMount>, String> {
    let mut validated = Vec::with_capacity(target_mounts.len());
    for host_mount in target_mounts {
        if read_only && host_mount.access == SpawnMountAccess::ReadWrite {
            return Err(format!(
                "read-only workstream `{stream_name}` cannot mount '{}' read_write",
                host_mount.target.display()
            ));
        }
        let Some(parent_mount) = parent_mounts
            .iter()
            .filter(|mount| host_mount.target.starts_with(Path::new(&mount.path)))
            .max_by_key(|mount| mount.path.len())
        else {
            return Err(format!(
                "workstream `{stream_name}` target mount '{}' has no live parent grant",
                host_mount.target.display()
            ));
        };
        if parent_mount.access == ProcessNamespaceAccess::ReadOnly
            && host_mount.access == SpawnMountAccess::ReadWrite
        {
            return Err(format!(
                "workstream `{stream_name}` mount '{}' cannot widen read_only grant '{}'",
                host_mount.target.display(),
                parent_mount.path
            ));
        }
        validated.push(host_mount.clone());
    }
    Ok(validated)
}

/// Validated Root dispatch plan produced from an inbound handoff message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffDispatchPlan {
    pub target_stream: String,
    pub next_hop_count: u8,
    pub focus_target: bool,
    pub correlation_id: String,
    pub payload: String,
    pub checkpoint_ref: Option<String>,
    pub authority: WorkstreamAuthorityProfile,
    pub receipt: RoutingReceipt,
}

/// Root coordinator disposition for one routed handoff or dead-letter record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffDisposition {
    /// Dispatch the work to `target_stream` under `authority` with `next_hop_count`.
    DispatchToStream(HandoffDispatchPlan),
    /// Deliver a `handoff_result` back to Root (no further child spawn required).
    DeliverResultToRoot {
        source_stream: String,
        target_stream: Option<String>,
        correlation_id: String,
        payload: String,
        receipt: RoutingReceipt,
    },
    /// Escalate to the user via Root without forwarding in a loop or widening authority.
    EscalateToUser {
        source_stream: String,
        suggested_target: Option<String>,
        correlation_id: String,
        hop_count: u8,
        reason: String,
        payload: String,
        receipt: RoutingReceipt,
    },
}

/// Stateful Root coordinator consuming `/mnt/route/ports/root` and `/mnt/route/ports/dead-letter`.
#[derive(Debug)]
pub struct WorkstreamHandoffCoordinator {
    route_base_path: String,
    root_cursor: RouteStreamCursor,
    dead_letter_cursor: RouteStreamCursor,
    correlation_hops: HashMap<String, u8>,
    correlation_order: VecDeque<String>,
}

impl Default for WorkstreamHandoffCoordinator {
    fn default() -> Self {
        Self::new(ROUTE_MOUNT_PATH)
    }
}

impl WorkstreamHandoffCoordinator {
    /// Construct a coordinator bound to `route_base_path` (usually `/mnt/route`).
    pub fn new(route_base_path: impl Into<String>) -> Self {
        let base = route_base_path.into().trim_end_matches('/').to_string();
        Self {
            root_cursor: RouteStreamCursor::new(format!("{base}/ports/{ROUTE_ROOT_PORT}")),
            dead_letter_cursor: RouteStreamCursor::new(format!(
                "{base}/ports/{ROUTE_DEAD_LETTER_PORT}"
            )),
            route_base_path: base,
            correlation_hops: HashMap::new(),
            correlation_order: VecDeque::new(),
        }
    }

    /// Ensure the default workstream handoff rules exist under `<route_base_path>/rules/`.
    pub async fn ensure_rules(&self, fs: &InProcessTransport) -> Result<()> {
        ensure_workstream_route_rules_at(fs, &self.route_base_path).await
    }

    /// Poll both `/mnt/route/ports/dead-letter` and `/mnt/route/ports/root`, resolving
    /// every new record against `ledger` and target-owned authority validation.
    pub async fn poll_and_resolve<F>(
        &mut self,
        fs: &InProcessTransport,
        ledger: &ProjectLedger,
        mut resolve_target_authority: F,
    ) -> Result<Vec<HandoffDisposition>>
    where
        F: FnMut(&WorkstreamRecord) -> std::result::Result<WorkstreamAuthorityProfile, String>,
    {
        let mut dispositions = Vec::new();
        for record in self.dead_letter_cursor.poll_records(fs).await? {
            if let Some(disposition) = self.resolve_dead_letter(&record) {
                dispositions.push(disposition);
            }
        }
        for record in self.root_cursor.poll_records(fs).await? {
            if let Some(handoff) = record.handoff_message() {
                dispositions.push(self.resolve_handoff(
                    handoff,
                    ledger,
                    &mut resolve_target_authority,
                ));
            }
        }
        Ok(dispositions)
    }

    fn resolve_dead_letter(&self, record: &RoutedRouteRecord) -> Option<HandoffDisposition> {
        let handoff = record.handoff_message()?;
        let reason = record
            .reason
            .clone()
            .unwrap_or_else(|| "routed to dead-letter".to_string());
        let kind = if record.is_loop_escalation() || handoff.exceeds_max_hops() {
            RoutingReceiptKind::LoopEscalation
        } else {
            RoutingReceiptKind::TargetUnavailable
        };
        let receipt = RoutingReceipt {
            kind,
            source_stream: handoff.source_stream.clone(),
            target_stream: handoff.suggested_target.clone(),
            correlation_id: handoff.correlation_id.clone(),
            hop_count: handoff.hop_count,
            detail: Some(reason.clone()),
        };
        Some(HandoffDisposition::EscalateToUser {
            source_stream: handoff.source_stream,
            suggested_target: handoff.suggested_target,
            correlation_id: handoff.correlation_id,
            hop_count: handoff.hop_count,
            reason,
            payload: handoff.payload,
            receipt,
        })
    }

    fn resolve_handoff<F>(
        &mut self,
        handoff: RouteHandoffMessage,
        ledger: &ProjectLedger,
        resolve_target_authority: &mut F,
    ) -> HandoffDisposition
    where
        F: FnMut(&WorkstreamRecord) -> std::result::Result<WorkstreamAuthorityProfile, String>,
    {
        if let Some(reason) = self.check_loop_bounds(&handoff) {
            return HandoffDisposition::EscalateToUser {
                source_stream: handoff.source_stream.clone(),
                suggested_target: handoff.suggested_target.clone(),
                correlation_id: handoff.correlation_id.clone(),
                hop_count: handoff.hop_count,
                reason: reason.clone(),
                payload: handoff.payload,
                receipt: RoutingReceipt {
                    kind: RoutingReceiptKind::LoopEscalation,
                    source_stream: handoff.source_stream,
                    target_stream: handoff.suggested_target,
                    correlation_id: handoff.correlation_id,
                    hop_count: handoff.hop_count,
                    detail: Some(reason),
                },
            };
        }

        match handoff.kind {
            RouteHandoffKind::HandoffResult => {
                let target_is_root = handoff
                    .suggested_target
                    .as_deref()
                    .is_none_or(|target| target == ROUTE_ROOT_PORT);
                if target_is_root {
                    return HandoffDisposition::DeliverResultToRoot {
                        source_stream: handoff.source_stream.clone(),
                        target_stream: Some(ROUTE_ROOT_PORT.to_string()),
                        correlation_id: handoff.correlation_id.clone(),
                        payload: handoff.payload,
                        receipt: RoutingReceipt {
                            kind: RoutingReceiptKind::HandoffResult,
                            source_stream: handoff.source_stream,
                            target_stream: Some(ROUTE_ROOT_PORT.to_string()),
                            correlation_id: handoff.correlation_id,
                            hop_count: handoff.hop_count,
                            detail: None,
                        },
                    };
                }
                self.dispatch_to_target(handoff, false, ledger, resolve_target_authority)
            }
            RouteHandoffKind::RerouteRequest => {
                self.dispatch_to_target(handoff, true, ledger, resolve_target_authority)
            }
            RouteHandoffKind::CrossStreamDependency => {
                self.dispatch_to_target(handoff, false, ledger, resolve_target_authority)
            }
        }
    }

    fn dispatch_to_target<F>(
        &self,
        handoff: RouteHandoffMessage,
        focus_target: bool,
        ledger: &ProjectLedger,
        resolve_target_authority: &mut F,
    ) -> HandoffDisposition
    where
        F: FnMut(&WorkstreamRecord) -> std::result::Result<WorkstreamAuthorityProfile, String>,
    {
        let receipt_kind = match handoff.kind {
            RouteHandoffKind::RerouteRequest => RoutingReceiptKind::Reroute,
            RouteHandoffKind::CrossStreamDependency => RoutingReceiptKind::CrossStreamDependency,
            RouteHandoffKind::HandoffResult => RoutingReceiptKind::HandoffResult,
        };
        let Some(target_name) = handoff.suggested_target.as_deref() else {
            return unavailable_escalation(
                handoff,
                "handoff did not specify a suggested_target workstream",
            );
        };
        if target_name == handoff.source_stream {
            return unavailable_escalation(
                handoff,
                "handoff target matches source_stream; escalating to avoid self-loop",
            );
        }
        let Some(target_record) = ledger.find_stream(target_name) else {
            let msg = format!("target workstream `{target_name}` is not in project ledger");
            return unavailable_escalation(handoff, &msg);
        };
        if matches!(
            target_record.state,
            WorkstreamState::Archived | WorkstreamState::PausedError
        ) {
            let msg = format!(
                "target workstream `{target_name}` is in state {:?}",
                target_record.state
            );
            return unavailable_escalation(handoff, &msg);
        }

        let authority = match resolve_target_authority(target_record) {
            Ok(authority) => authority,
            Err(reason) => return unavailable_escalation(handoff, &reason),
        };
        if authority.stream_name != target_record.name {
            return unavailable_escalation(
                handoff,
                &format!(
                    "authority profile stream mismatch: expected `{}`, got `{}`",
                    target_record.name, authority.stream_name
                ),
            );
        }

        let next_hop_count = handoff.hop_count.saturating_add(1);
        let receipt = RoutingReceipt {
            kind: receipt_kind,
            source_stream: handoff.source_stream,
            target_stream: Some(target_record.name.clone()),
            correlation_id: handoff.correlation_id.clone(),
            hop_count: handoff.hop_count,
            detail: None,
        };
        HandoffDisposition::DispatchToStream(HandoffDispatchPlan {
            target_stream: target_record.name.clone(),
            next_hop_count,
            focus_target,
            correlation_id: handoff.correlation_id,
            payload: handoff.payload,
            checkpoint_ref: target_record.last_checkpoint_ref.clone(),
            authority,
            receipt,
        })
    }

    fn check_loop_bounds(&mut self, handoff: &RouteHandoffMessage) -> Option<String> {
        if handoff.exceeds_max_hops() {
            return Some(format!(
                "handoff hop_count {} exceeds max_handoff_hops={MAX_HANDOFF_HOPS}",
                handoff.hop_count
            ));
        }
        let is_terminal_root_result = handoff.kind == RouteHandoffKind::HandoffResult
            && handoff
                .suggested_target
                .as_deref()
                .is_none_or(|target| target == ROUTE_ROOT_PORT);
        if is_terminal_root_result {
            return None;
        }
        let seen_count = self
            .correlation_hops
            .get(&handoff.correlation_id)
            .copied()
            .unwrap_or(0);
        if seen_count > MAX_HANDOFF_HOPS {
            return Some(format!(
                "correlation `{}` exceeded max_handoff_hops={MAX_HANDOFF_HOPS} (seen {seen_count} hops)",
                handoff.correlation_id
            ));
        }
        let updated = seen_count.saturating_add(1);
        if !self.correlation_hops.contains_key(&handoff.correlation_id) {
            if self.correlation_order.len() >= MAX_TRACKED_CORRELATIONS
                && let Some(oldest) = self.correlation_order.pop_front()
            {
                self.correlation_hops.remove(&oldest);
            }
            self.correlation_order
                .push_back(handoff.correlation_id.clone());
        }
        self.correlation_hops
            .insert(handoff.correlation_id.clone(), updated);
        None
    }
}

fn unavailable_escalation(handoff: RouteHandoffMessage, reason: &str) -> HandoffDisposition {
    HandoffDisposition::EscalateToUser {
        source_stream: handoff.source_stream.clone(),
        suggested_target: handoff.suggested_target.clone(),
        correlation_id: handoff.correlation_id.clone(),
        hop_count: handoff.hop_count,
        reason: reason.to_string(),
        payload: handoff.payload,
        receipt: RoutingReceipt {
            kind: RoutingReceiptKind::TargetUnavailable,
            source_stream: handoff.source_stream,
            target_stream: handoff.suggested_target,
            correlation_id: handoff.correlation_id,
            hop_count: handoff.hop_count,
            detail: Some(reason.to_string()),
        },
    }
}
