use alan_agent_protocol::ToolCapability;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::agent_machine::AgentMachine;
use crate::rollout::{EffectRecord, EffectStatus, build_durable_tool_payload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EffectCategory {
    File,
    Network,
    Process,
}

impl EffectCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Network => "network",
            Self::Process => "process",
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct EffectIdentity {
    pub(super) category: EffectCategory,
    pub(super) idempotency_key: String,
    pub(super) request_fingerprint: String,
}

#[derive(Debug, Clone)]
pub(super) enum ToolEffectPlan {
    ConfirmUnknown,
    ReplayApplied { payload: Value },
    Execute,
}

#[derive(Debug)]
pub(super) struct EffectCheckpointFailure {
    pub(super) message: String,
    pub(super) payload: Value,
}

/// Owns one effectful Tool call from stable identity through its terminal durable record.
/// Policy evaluation stays with the orchestrator; physical execution is driven by the narrow
/// Tool execution transition owner.
#[derive(Debug, Clone)]
pub(super) struct ToolEffectLifecycle {
    identity: EffectIdentity,
    existing: Option<EffectRecord>,
    process_path: String,
    tool_call_id: String,
    tool_name: String,
}

impl ToolEffectLifecycle {
    pub(super) fn for_call(
        machine: &AgentMachine,
        process_path: impl Into<String>,
        tool_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        tool_arguments: &Value,
        capability: ToolCapability,
    ) -> Option<Self> {
        let tool_name = tool_name.into();
        let category = classify_effect_category(&tool_name, capability)?;
        let identity = build_effect_identity(machine, &tool_name, tool_arguments, category);
        let existing = machine.effect_by_idempotency_key(&identity.idempotency_key);

        Some(Self {
            identity,
            existing,
            process_path: process_path.into(),
            tool_call_id: tool_call_id.into(),
            tool_name,
        })
    }

    pub(super) fn effect_type(&self) -> &'static str {
        self.identity.category.as_str()
    }

    pub(super) fn idempotency_key(&self) -> &str {
        &self.identity.idempotency_key
    }

    pub(super) fn request_fingerprint(&self) -> &str {
        &self.identity.request_fingerprint
    }

    pub(super) fn plan(
        &self,
        machine: &AgentMachine,
        allow_unknown_execution: bool,
    ) -> ToolEffectPlan {
        match self.existing.as_ref() {
            Some(existing)
                if matches!(existing.status, EffectStatus::Unknown) && !allow_unknown_execution =>
            {
                ToolEffectPlan::ConfirmUnknown
            }
            Some(existing) if matches!(existing.status, EffectStatus::Applied) => {
                let payload = machine
                    .tool_payload_by_call_id(&existing.tool_call_id)
                    .or_else(|| existing.result_payload.clone())
                    .unwrap_or_else(|| {
                        json!({
                            "status": "dedupe_hit",
                            "dedupe_hit": true,
                            "reason": "Matching applied side effect found; skipped physical execution",
                            "idempotency_key": self.idempotency_key(),
                            "effect_type": self.effect_type(),
                            "effect_status": "applied"
                        })
                    });
                ToolEffectPlan::ReplayApplied { payload }
            }
            _ => ToolEffectPlan::Execute,
        }
    }

    pub(super) fn record_unknown_confirmation(&self, machine: &mut AgentMachine, reason: &str) {
        self.record_decision(machine, "escalate", Some(reason), false, true);
    }

    pub(super) fn record_execute_decision(&self, machine: &mut AgentMachine, reason: &str) {
        self.record_decision(machine, "execute", Some(reason), false, false);
    }

    pub(super) fn commit_replay(&self, machine: &mut AgentMachine, payload: &Value, reason: &str) {
        let existing = self
            .existing
            .as_ref()
            .expect("replay plan requires an applied effect record");
        self.record_decision(machine, "skip", Some(reason), true, true);

        let now = chrono::Utc::now().to_rfc3339();
        let durable_payload = build_durable_tool_payload(payload);
        let result_digest = existing
            .result_digest
            .clone()
            .unwrap_or_else(|| durable_payload.digest.clone());
        machine.record_effect(EffectRecord {
            effect_id: new_effect_id(),
            process_path: self.process_path.clone(),
            tool_call_id: self.tool_call_id.clone(),
            idempotency_key: self.identity.idempotency_key.clone(),
            effect_type: self.effect_type().to_string(),
            request_fingerprint: self.identity.request_fingerprint.clone(),
            result_digest: Some(result_digest),
            result_payload: Some(durable_payload.payload),
            status: EffectStatus::Applied,
            applied_at: existing.applied_at.clone().or(Some(now.clone())),
            reason: Some(reason.to_string()),
            dedupe_hit: true,
            timestamp: now,
        });
    }

    pub(super) async fn begin(
        &self,
        machine: &mut AgentMachine,
    ) -> std::result::Result<EffectRecord, EffectCheckpointFailure> {
        let record = EffectRecord {
            effect_id: new_effect_id(),
            process_path: self.process_path.clone(),
            tool_call_id: self.tool_call_id.clone(),
            idempotency_key: self.identity.idempotency_key.clone(),
            effect_type: self.effect_type().to_string(),
            request_fingerprint: self.identity.request_fingerprint.clone(),
            result_digest: None,
            result_payload: None,
            status: EffectStatus::Unknown,
            applied_at: None,
            reason: Some("execution started before terminal status commit".to_string()),
            dedupe_hit: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        machine.record_effect(record.clone());

        let flush_result = machine.flush_recorder().await;
        if let Err(error) = flush_result {
            let message = format!("Failed to persist side-effect checkpoint: {error}");
            let payload = json!({
                "error": message,
                "status": "effect_checkpoint_persist_failed"
            });
            self.complete(machine, &record, &payload, false, Some(message.clone()));
            return Err(EffectCheckpointFailure { message, payload });
        }

        Ok(record)
    }

    pub(super) fn complete(
        &self,
        machine: &mut AgentMachine,
        started: &EffectRecord,
        payload: &Value,
        success: bool,
        reason: Option<String>,
    ) {
        let durable_payload = build_durable_tool_payload(payload);
        machine.record_effect(EffectRecord {
            effect_id: started.effect_id.clone(),
            process_path: started.process_path.clone(),
            tool_call_id: started.tool_call_id.clone(),
            idempotency_key: self.identity.idempotency_key.clone(),
            effect_type: self.effect_type().to_string(),
            request_fingerprint: self.identity.request_fingerprint.clone(),
            result_digest: Some(durable_payload.digest),
            result_payload: Some(durable_payload.payload),
            status: if success {
                EffectStatus::Applied
            } else {
                EffectStatus::Failed
            },
            applied_at: success.then(|| chrono::Utc::now().to_rfc3339()),
            reason,
            dedupe_hit: false,
            timestamp: chrono::Utc::now().to_rfc3339(),
        });
    }

    fn record_decision(
        &self,
        machine: &mut AgentMachine,
        decision: &str,
        reason: Option<&str>,
        dedupe_hit: bool,
        include_existing_effect_id: bool,
    ) {
        let mut event = json!({
            "process_path": self.process_path,
            "tool_call_id": self.tool_call_id,
            "tool_name": self.tool_name,
            "effect_type": self.effect_type(),
            "idempotency_key": self.idempotency_key(),
            "request_fingerprint": self.request_fingerprint(),
            "decision": effect_decision_reason(
                decision,
                reason,
                self.existing.as_ref().map(|effect| &effect.status),
                dedupe_hit,
            )
        });
        if include_existing_effect_id && let Some(existing) = self.existing.as_ref() {
            event["existing_effect_id"] = Value::String(existing.effect_id.clone());
        }
        machine.record_event("effect_dedupe_decision", event);
    }
}

fn classify_effect_category(
    tool_name: &str,
    tool_capability: ToolCapability,
) -> Option<EffectCategory> {
    match tool_capability {
        ToolCapability::Read => None,
        ToolCapability::Network => Some(EffectCategory::Network),
        ToolCapability::Write => {
            if matches!(tool_name, "write_file" | "edit_file") {
                Some(EffectCategory::File)
            } else {
                Some(EffectCategory::Process)
            }
        }
        ToolCapability::Unknown => (tool_name == "bash").then_some(EffectCategory::Process),
    }
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort();
            let mut sorted = Map::new();
            for key in keys {
                if let Some(entry) = map.get(key) {
                    sorted.insert(key.clone(), canonicalize_json(entry));
                }
            }
            Value::Object(sorted)
        }
        Value::Array(items) => Value::Array(items.iter().map(canonicalize_json).collect()),
        _ => value.clone(),
    }
}

pub(super) fn build_effect_identity(
    machine: &AgentMachine,
    tool_name: &str,
    tool_arguments: &Value,
    category: EffectCategory,
) -> EffectIdentity {
    let request_payload = json!({
        "tool_name": tool_name,
        "effect_type": category.as_str(),
        "arguments": canonicalize_json(tool_arguments),
    });
    let request_fingerprint = sha256_hex(&request_payload.to_string());
    // The Agent Machine carries this index across recovery. Process paths remain
    // EffectRecord provenance, but cannot identify the effect because recovery
    // intentionally starts a fresh Process with a different PID.
    let idempotency_key = format!(
        "machine:turn:{}:{}",
        machine.user_turn_ordinal(),
        request_fingerprint
    );

    EffectIdentity {
        category,
        idempotency_key,
        request_fingerprint,
    }
}

fn effect_decision_reason(
    decision: &str,
    reason: Option<&str>,
    existing_status: Option<&EffectStatus>,
    dedupe_hit: bool,
) -> Value {
    json!({
        "decision": decision,
        "reason": reason,
        "existing_status": existing_status.map(|status| match status {
            EffectStatus::Applied => "applied",
            EffectStatus::Failed => "failed",
            EffectStatus::Unknown => "unknown",
        }),
        "dedupe_hit": dedupe_hit,
    })
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

fn new_effect_id() -> String {
    format!("ef-{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
#[path = "tool_effect_lifecycle_tests.rs"]
mod tests;
