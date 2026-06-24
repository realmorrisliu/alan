//! Built-in Alan Agent app projection layer for Alan OS.
//!
//! This crate maps current daemon-backed Alan Agent sessions and protocol events
//! into Alan Kernel semantic objects, buffers, views, commands, tasks, artifacts,
//! and evidence. It does not own provider execution, daemon sessions, memory
//! stores, sandbox execution, or terminal rendering.

use crate::model::*;
use alan_kernel::{
    ActorDescriptor, ActorKind, ArtifactDescriptor, ArtifactId, BufferDescriptor, BufferId,
    BufferKind, BufferSource, CommandDescriptor, CommandId, CommandInvocation,
    CommandRecoveryPolicy, CommandRisk, CommandTarget, DescriptorMetadata, EventId,
    EvidenceDescriptor, EvidenceId, FileReference, FormField, FormFieldKind, FormViewModel,
    InvocationHintMetadata, InvocationSurface, KernelEvent, KernelEventKind, LogEntry,
    LogStreamViewModel, NativeReference, ObjectDescriptor, ObjectKind, ObjectListItem,
    ObjectListViewModel, TaskDescriptor, TaskEvent, TaskEventKind, TaskFailure, TaskId,
    TaskOutputChunk, TaskOutputStream, TaskProgress, TaskSideEffect, TaskSideEffectKind,
    TaskStatus, TaskTreeNode, TaskTreeViewModel, TaskYieldCheckpoint, TaskYieldKind, ViewAction,
    ViewDescriptor, ViewId, ViewKind, ViewModel, ViewSemanticState, ViewSnapshot,
};
use alan_kernel::{AgentSessionReference, ConversationBlock, ConversationBlockKind};
use alan_protocol::{
    Event, EventEnvelope, MemoryFlushResult, PlanItemStatus, ToolResultPresentation, YieldKind,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Mutable projection of current Alan Agent compatibility events.
#[derive(Clone, Debug)]
pub struct AgentWorkspaceProjector {
    ids: AgentWorkspaceIds,
    session: AgentWorkspaceSessionMetadata,
    sequence: u64,
    kernel_events: Vec<KernelEvent>,
    conversation_blocks: Vec<ConversationBlock>,
    tasks: BTreeMap<TaskId, AgentWorkspaceTaskEntry>,
    turn_tasks: BTreeMap<String, TaskId>,
    tool_tasks: BTreeMap<String, TaskId>,
    active_turn_task: Option<TaskId>,
    evidence_items: Vec<ObjectListItem>,
    artifact_items: Vec<ObjectListItem>,
    memory_items: Vec<ObjectListItem>,
    audit_entries: Vec<LogEntry>,
    pending_form: Option<FormViewModel>,
    snapshot_version: u64,
}

impl AgentWorkspaceProjector {
    /// Creates a projector and emits initial object, buffer, view, and command availability events.
    #[must_use]
    pub fn new(session: AgentWorkspaceSessionMetadata) -> Self {
        let ids = AgentWorkspaceIds::default();
        let mut projector = Self {
            ids,
            session,
            sequence: 0,
            kernel_events: Vec::new(),
            conversation_blocks: Vec::new(),
            tasks: BTreeMap::new(),
            turn_tasks: BTreeMap::new(),
            tool_tasks: BTreeMap::new(),
            active_turn_task: None,
            evidence_items: Vec::new(),
            artifact_items: Vec::new(),
            memory_items: Vec::new(),
            audit_entries: Vec::new(),
            pending_form: None,
            snapshot_version: 1,
        };
        projector.emit_initial_model_events();
        projector
    }

    /// Returns ids allocated for this projection.
    #[must_use]
    pub fn ids(&self) -> &AgentWorkspaceIds {
        &self.ids
    }

    /// Returns the static model descriptors for this workspace.
    #[must_use]
    pub fn model(&self) -> AgentWorkspaceModel {
        AgentWorkspaceModel {
            actors: self.actor_descriptors(),
            objects: self.object_descriptors(),
            buffers: self.buffer_descriptors(),
            views: self.view_descriptors(),
            commands: self.command_descriptors(),
        }
    }

    /// Returns all Kernel events emitted so far.
    #[must_use]
    pub fn kernel_events(&self) -> &[KernelEvent] {
        &self.kernel_events
    }

    /// Projects one current protocol event envelope into Agent Workspace semantics.
    pub fn apply_envelope(&mut self, envelope: &EventEnvelope) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        self.apply_protocol_event(envelope);
        self.snapshot_version += 1;
        self.kernel_events[before..].to_vec()
    }

    /// Projects a locally submitted user turn before the daemon emits turn events.
    pub fn apply_user_submission(&mut self, text: impl Into<String>) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        if self.project_user_submission(text.into()) {
            self.snapshot_version += 1;
        }
        self.kernel_events[before..].to_vec()
    }

    /// Projects one recovered compatibility history message into semantic conversation state.
    pub fn apply_hydrated_message(
        &mut self,
        message: AgentWorkspaceHydratedMessage,
    ) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        if self.project_hydrated_message(message) {
            self.snapshot_version += 1;
            self.invalidate_view(
                self.ids.surfaces.conversation_view,
                "history message hydrated",
            );
        }
        self.kernel_events[before..].to_vec()
    }

    /// Clears visible conversation projection while keeping session/task model state intact.
    pub fn clear_transcript(&mut self) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        self.conversation_blocks.clear();
        self.snapshot_version += 1;
        self.invalidate_view(self.ids.surfaces.conversation_view, "conversation cleared");
        self.kernel_events[before..].to_vec()
    }

    /// Clears the current pending yield form after the host accepts a resume.
    pub fn clear_pending_yield(&mut self) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        if self.pending_form.take().is_some() {
            self.snapshot_version += 1;
            self.invalidate_view(
                self.ids.surfaces.approval_form_view,
                "yield checkpoint resumed",
            );
        }
        self.kernel_events[before..].to_vec()
    }

    /// Projects a current child run or delegated skill lifecycle event.
    pub fn apply_child_run_event(
        &mut self,
        child_run: AgentWorkspaceChildRunEvent,
    ) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        self.project_child_run_event(child_run);
        self.snapshot_version += 1;
        self.kernel_events[before..].to_vec()
    }

    /// Projects memory recall, promotion, or flush activity into memory review.
    pub fn apply_memory_observation(
        &mut self,
        observation: AgentWorkspaceMemoryObservation,
    ) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        self.project_memory_observation(observation);
        self.snapshot_version += 1;
        self.kernel_events[before..].to_vec()
    }

    /// Projects rollout artifacts, effects, checkpoints, and evidence records.
    pub fn apply_rollout_record(
        &mut self,
        record: AgentWorkspaceRolloutRecord,
    ) -> Vec<KernelEvent> {
        let before = self.kernel_events.len();
        self.project_rollout_record(record);
        self.snapshot_version += 1;
        self.kernel_events[before..].to_vec()
    }

    /// Returns current semantic view snapshots.
    #[must_use]
    pub fn snapshots(&self) -> AgentWorkspaceSnapshots {
        AgentWorkspaceSnapshots {
            conversation: self.conversation_snapshot(),
            task_tree: self.task_tree_snapshot(),
            evidence: self.evidence_snapshot(),
            memory_review: self.memory_review_snapshot(),
            approval_form: self.approval_form_snapshot(),
            command_palette: self.command_palette_snapshot(),
            audit: self.audit_snapshot(),
        }
    }

    fn emit_initial_model_events(&mut self) {
        for object in self.object_descriptors() {
            self.push_event(KernelEventKind::ObjectUpserted { descriptor: object });
        }
        for buffer in self.buffer_descriptors() {
            self.push_event(KernelEventKind::BufferUpserted { descriptor: buffer });
        }
        for view in self.view_descriptors() {
            self.push_event(KernelEventKind::ViewUpserted { descriptor: view });
        }
        self.push_event(KernelEventKind::CommandAvailabilityChanged {
            target: CommandTarget::Object {
                id: self.ids.objects.compatibility_session,
            },
            command_ids: vec![
                self.ids.commands.submit_turn,
                self.ids.commands.interrupt,
                self.ids.commands.compact,
                self.ids.commands.rollback,
            ],
        });
        self.push_event(KernelEventKind::CommandAvailabilityChanged {
            target: CommandTarget::View {
                id: self.ids.surfaces.command_palette_view,
            },
            command_ids: self
                .command_descriptors()
                .iter()
                .map(|command| command.id)
                .collect(),
        });
    }

    fn apply_protocol_event(&mut self, envelope: &EventEnvelope) {
        self.audit(envelope.timestamp_ms, protocol_event_label(&envelope.event));
        match &envelope.event {
            Event::TurnStarted {} => self.start_turn(envelope),
            Event::TurnCompleted { summary } => self.complete_turn(envelope, summary.clone()),
            Event::TextDelta { chunk, is_final } => {
                self.append_conversation_block(
                    envelope.item_id.clone(),
                    ConversationBlockKind::Assistant,
                    chunk,
                    true,
                );
                self.append_task_output(TaskOutputStream::Text, chunk.clone(), *is_final);
                self.invalidate_main_views("assistant text updated");
            }
            Event::ThinkingDelta { chunk, is_final } => {
                self.append_conversation_block(
                    envelope.item_id.clone(),
                    ConversationBlockKind::Thinking,
                    chunk,
                    true,
                );
                self.append_task_output(TaskOutputStream::Thinking, chunk.clone(), *is_final);
                self.invalidate_main_views("thinking updated");
            }
            Event::ToolCallStarted {
                id, name, title, ..
            } => self.start_tool_call(envelope, id, name, title.as_deref()),
            Event::ToolCallCompleted {
                id,
                name,
                success,
                result_preview,
                presentation,
                ..
            } => self.complete_tool_call(
                envelope,
                id,
                name.as_deref(),
                *success,
                result_preview.as_deref(),
                presentation.as_ref(),
            ),
            Event::PlanUpdated { explanation, items } => {
                self.record_plan(envelope, explanation.as_deref(), items);
            }
            Event::SessionRolledBack { turns, .. } => {
                self.progress_current_task(
                    format!("rolled back {turns} turns"),
                    Some("rollback".to_string()),
                );
                self.invalidate_main_views("session rolled back");
            }
            Event::Yield {
                request_id,
                kind,
                payload,
            } => self.record_yield(envelope, request_id, kind, payload),
            Event::CompactionObserved { attempt } => {
                self.record_evidence(
                    envelope,
                    "Compaction attempt",
                    json!({
                        "kind": "compaction",
                        "result": attempt.result,
                        "trigger": attempt.request.trigger,
                        "reason": attempt.request.reason,
                    }),
                    None,
                );
                self.progress_current_task(
                    "context compaction observed",
                    Some("compaction".into()),
                );
            }
            Event::MemoryFlushObserved { attempt } => {
                self.record_memory_flush(envelope, attempt);
            }
            Event::Warning { message } => {
                self.progress_current_task(format!("warning: {message}"), Some("warning".into()));
            }
            Event::Error {
                message,
                recoverable,
            } => self.record_error(envelope, message, *recoverable),
        }
    }

    fn actor_descriptors(&self) -> Vec<ActorDescriptor> {
        vec![
            ActorDescriptor {
                id: self.ids.user_actor,
                kind: ActorKind::Human,
                metadata: DescriptorMetadata::new("User"),
                native_ref: None,
            },
            ActorDescriptor {
                id: self.ids.agent_actor,
                kind: ActorKind::Agent,
                metadata: DescriptorMetadata::new("Alan Agent"),
                native_ref: Some(self.session_native_ref()),
            },
            ActorDescriptor {
                id: self.ids.system_actor,
                kind: ActorKind::System,
                metadata: DescriptorMetadata::new("Alan Agent Workspace Projection"),
                native_ref: None,
            },
        ]
    }

    fn object_descriptors(&self) -> Vec<ObjectDescriptor> {
        vec![
            self.object_descriptor(
                self.ids.objects.compatibility_session,
                AgentWorkspaceObjectRole::CompatibilitySession,
                ObjectKind::AgentSession,
                "Compatibility Session",
                Some(self.session_native_ref()),
            ),
            self.object_descriptor(
                self.ids.objects.agent_run,
                AgentWorkspaceObjectRole::AgentRun,
                ObjectKind::Synthetic,
                "Current Agent Run",
                None,
            ),
            self.object_descriptor(
                self.ids.objects.supervisor_tasks,
                AgentWorkspaceObjectRole::SupervisorTaskInbox,
                ObjectKind::Synthetic,
                "Supervisor Tasks",
                None,
            ),
            self.object_descriptor(
                self.ids.objects.memory_entries,
                AgentWorkspaceObjectRole::MemoryEntries,
                ObjectKind::Synthetic,
                "Memory Review",
                None,
            ),
            self.object_descriptor(
                self.ids.objects.evidence,
                AgentWorkspaceObjectRole::Evidence,
                ObjectKind::Synthetic,
                "Evidence",
                None,
            ),
            self.object_descriptor(
                self.ids.objects.artifacts,
                AgentWorkspaceObjectRole::Artifacts,
                ObjectKind::Synthetic,
                "Artifacts",
                None,
            ),
            self.object_descriptor(
                self.ids.objects.plans,
                AgentWorkspaceObjectRole::Plans,
                ObjectKind::Synthetic,
                "Plans",
                None,
            ),
        ]
    }

    fn object_descriptor(
        &self,
        id: alan_kernel::ObjectId,
        role: AgentWorkspaceObjectRole,
        kind: ObjectKind,
        title: &str,
        native_ref: Option<NativeReference>,
    ) -> ObjectDescriptor {
        let mut metadata = DescriptorMetadata::new(title);
        metadata.tags = vec!["alan-agent".to_string(), role.as_str().to_string()];
        metadata.attributes = session_attributes(&self.session);
        metadata
            .attributes
            .insert("workspace_role".to_string(), json!(role));
        ObjectDescriptor {
            id,
            kind,
            metadata,
            native_ref,
            capabilities: vec!["agent.workspace.inspect".to_string()],
        }
    }

    fn buffer_descriptors(&self) -> Vec<BufferDescriptor> {
        vec![
            self.object_buffer(
                self.ids.surfaces.conversation_buffer,
                self.ids.objects.compatibility_session,
                "Conversation",
            ),
            self.object_buffer(
                self.ids.surfaces.task_tree_buffer,
                self.ids.objects.agent_run,
                "Tasks",
            ),
            self.object_buffer(
                self.ids.surfaces.evidence_buffer,
                self.ids.objects.evidence,
                "Evidence",
            ),
            self.object_buffer(
                self.ids.surfaces.memory_review_buffer,
                self.ids.objects.memory_entries,
                "Memory Review",
            ),
            BufferDescriptor {
                id: self.ids.surfaces.approval_form_buffer,
                kind: BufferKind::Scratch,
                source: BufferSource::Scratch,
                metadata: DescriptorMetadata::new("Approval"),
            },
            BufferDescriptor {
                id: self.ids.surfaces.command_palette_buffer,
                kind: BufferKind::Scratch,
                source: BufferSource::Scratch,
                metadata: DescriptorMetadata::new("Command Palette"),
            },
            self.object_buffer(
                self.ids.surfaces.audit_buffer,
                self.ids.objects.compatibility_session,
                "Audit",
            ),
        ]
    }

    fn object_buffer(
        &self,
        id: BufferId,
        object_id: alan_kernel::ObjectId,
        title: &str,
    ) -> BufferDescriptor {
        BufferDescriptor {
            id,
            kind: BufferKind::Object,
            source: BufferSource::Object { id: object_id },
            metadata: DescriptorMetadata::new(title),
        }
    }

    fn view_descriptors(&self) -> Vec<ViewDescriptor> {
        vec![
            self.view_descriptor(
                self.ids.surfaces.conversation_view,
                self.ids.surfaces.conversation_buffer,
                ViewKind::Conversation,
                "Conversation",
            ),
            self.view_descriptor(
                self.ids.surfaces.task_tree_view,
                self.ids.surfaces.task_tree_buffer,
                ViewKind::TaskTree,
                "Task Tree",
            ),
            self.view_descriptor(
                self.ids.surfaces.evidence_view,
                self.ids.surfaces.evidence_buffer,
                ViewKind::ObjectList,
                "Evidence",
            ),
            self.view_descriptor(
                self.ids.surfaces.memory_review_view,
                self.ids.surfaces.memory_review_buffer,
                ViewKind::ObjectList,
                "Memory Review",
            ),
            self.view_descriptor(
                self.ids.surfaces.approval_form_view,
                self.ids.surfaces.approval_form_buffer,
                ViewKind::Form,
                "Approval",
            ),
            self.view_descriptor(
                self.ids.surfaces.command_palette_view,
                self.ids.surfaces.command_palette_buffer,
                ViewKind::CommandPalette,
                "Command Palette",
            ),
            self.view_descriptor(
                self.ids.surfaces.audit_view,
                self.ids.surfaces.audit_buffer,
                ViewKind::LogStream,
                "Audit",
            ),
        ]
    }

    fn view_descriptor(
        &self,
        id: ViewId,
        buffer_id: BufferId,
        kind: ViewKind,
        title: &str,
    ) -> ViewDescriptor {
        ViewDescriptor {
            id,
            buffer_id,
            kind,
            metadata: DescriptorMetadata::new(title),
        }
    }

    fn command_descriptors(&self) -> Vec<CommandDescriptor> {
        vec![
            self.command(
                self.ids.commands.submit_turn,
                "agent.submit_turn",
                CommandTarget::Buffer {
                    id: self.ids.surfaces.conversation_buffer,
                },
                "Submit Turn",
                CommandRisk::Medium,
                &["submit", "send", "turn"],
            ),
            self.command(
                self.ids.commands.resume_yield,
                "agent.resume_yield",
                CommandTarget::View {
                    id: self.ids.surfaces.approval_form_view,
                },
                "Resume Yield",
                CommandRisk::Medium,
                &["resume", "continue"],
            ),
            self.command(
                self.ids.commands.approve_command,
                "agent.approve_command",
                CommandTarget::View {
                    id: self.ids.surfaces.approval_form_view,
                },
                "Approve",
                CommandRisk::High,
                &["approve", "yes"],
            ),
            self.command(
                self.ids.commands.deny_command,
                "agent.deny_command",
                CommandTarget::View {
                    id: self.ids.surfaces.approval_form_view,
                },
                "Deny",
                CommandRisk::Low,
                &["deny", "reject", "no"],
            ),
            self.command(
                self.ids.commands.interrupt,
                "agent.interrupt",
                CommandTarget::Object {
                    id: self.ids.objects.compatibility_session,
                },
                "Interrupt",
                CommandRisk::Medium,
                &["interrupt", "stop"],
            ),
            self.command(
                self.ids.commands.compact,
                "agent.compact",
                CommandTarget::Object {
                    id: self.ids.objects.compatibility_session,
                },
                "Compact Context",
                CommandRisk::Medium,
                &["compact", "summarize context"],
            ),
            self.command(
                self.ids.commands.rollback,
                "agent.rollback",
                CommandTarget::Object {
                    id: self.ids.objects.compatibility_session,
                },
                "Rollback",
                CommandRisk::High,
                &["rollback", "undo turn"],
            ),
            self.command(
                self.ids.commands.inspect_evidence,
                "agent.inspect_evidence",
                CommandTarget::Object {
                    id: self.ids.objects.evidence,
                },
                "Inspect Evidence",
                CommandRisk::ReadOnly,
                &["evidence", "inspect"],
            ),
            self.command(
                self.ids.commands.promote_supervisor_task,
                "agent.promote_supervisor_task",
                CommandTarget::Object {
                    id: self.ids.objects.supervisor_tasks,
                },
                "Promote Supervisor Task",
                CommandRisk::Medium,
                &["promote task", "open task"],
            ),
            self.command(
                self.ids.commands.open_memory_review,
                "agent.open_memory_review",
                CommandTarget::Object {
                    id: self.ids.objects.memory_entries,
                },
                "Open Memory Review",
                CommandRisk::ReadOnly,
                &["memory", "review memory"],
            ),
        ]
    }

    fn command(
        &self,
        id: CommandId,
        name: &str,
        target: CommandTarget,
        title: &str,
        risk: CommandRisk,
        aliases: &[&str],
    ) -> CommandDescriptor {
        CommandDescriptor {
            id,
            name: name.to_string(),
            target,
            args_schema: Some(json!({"type": "object"})),
            required_capabilities: Vec::new(),
            risk,
            recovery: CommandRecoveryPolicy::Retryable,
            invocation_hints: InvocationHintMetadata {
                preferred_surfaces: vec![InvocationSurface::CommandPalette],
                aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
                keyboard_shortcuts: Vec::new(),
                confirmation: None,
                attributes: BTreeMap::new(),
            },
            metadata: DescriptorMetadata::new(title),
        }
    }

    fn project_user_submission(&mut self, text: String) -> bool {
        if text.is_empty() {
            return false;
        }
        self.conversation_blocks.push(ConversationBlock {
            id: format!("local-user:{}", self.conversation_blocks.len()),
            kind: ConversationBlockKind::User,
            text: text.clone(),
            task_id: None,
            artifact_id: None,
            evidence_ids: Vec::new(),
        });
        if let Some(descriptor) = self
            .command_descriptors()
            .into_iter()
            .find(|command| command.id == self.ids.commands.submit_turn)
        {
            self.push_event(KernelEventKind::CommandInvoked {
                invocation: CommandInvocation::from_descriptor(
                    &descriptor,
                    self.ids.user_actor,
                    json!({ "text": text }),
                ),
            });
        }
        self.invalidate_main_views("user submission projected");
        true
    }

    fn project_hydrated_message(&mut self, message: AgentWorkspaceHydratedMessage) -> bool {
        if message.content.is_empty() {
            return false;
        }
        let (kind, text) = match message.role.as_str() {
            "user" => (ConversationBlockKind::User, message.content),
            "assistant" => (ConversationBlockKind::Assistant, message.content),
            "tool" => {
                let text = message
                    .tool_name
                    .map(|tool_name| format!("{tool_name}: {}", message.content))
                    .unwrap_or(message.content);
                (ConversationBlockKind::Tool, text)
            }
            _ => return false,
        };
        self.conversation_blocks.push(ConversationBlock {
            id: format!(
                "hydrated:{}:{}",
                message.role,
                self.conversation_blocks.len()
            ),
            kind,
            text,
            task_id: None,
            artifact_id: None,
            evidence_ids: Vec::new(),
        });
        true
    }

    fn start_turn(&mut self, envelope: &EventEnvelope) {
        let task_id = TaskId::new();
        self.turn_tasks.insert(envelope.turn_id.clone(), task_id);
        self.active_turn_task = Some(task_id);
        let descriptor = TaskDescriptor {
            id: task_id,
            actor_id: self.ids.agent_actor,
            parent_task_id: None,
            command_id: Some(self.ids.commands.submit_turn),
            status: TaskStatus::Running,
            metadata: DescriptorMetadata::new(format!("Turn {}", envelope.turn_id)),
        };
        self.tasks.insert(
            task_id,
            AgentWorkspaceTaskEntry::from_descriptor(&descriptor, None),
        );
        self.push_task_event(task_id, TaskEventKind::Started { descriptor });
        self.invalidate_main_views("turn started");
    }

    fn complete_turn(&mut self, _envelope: &EventEnvelope, summary: Option<String>) {
        let Some(task_id) = self.active_turn_task.take() else {
            return;
        };
        if let Some(task) = self.tasks.get_mut(&task_id) {
            task.status = "completed".to_string();
        }
        self.push_task_event(
            task_id,
            TaskEventKind::Completed {
                summary,
                artifact_ids: Vec::new(),
                evidence_ids: Vec::new(),
            },
        );
        self.invalidate_main_views("turn completed");
    }

    fn start_tool_call(
        &mut self,
        envelope: &EventEnvelope,
        tool_call_id: &str,
        name: &str,
        title: Option<&str>,
    ) {
        let parent_task_id = self.active_turn_task;
        let task_id = TaskId::new();
        self.tool_tasks.insert(tool_call_id.to_string(), task_id);
        let label = title.unwrap_or(name).to_string();
        let descriptor = TaskDescriptor {
            id: task_id,
            actor_id: self.ids.agent_actor,
            parent_task_id,
            command_id: None,
            status: TaskStatus::Running,
            metadata: DescriptorMetadata::new(label.clone()),
        };
        self.tasks.insert(
            task_id,
            AgentWorkspaceTaskEntry::from_descriptor(&descriptor, parent_task_id),
        );
        self.push_task_event(task_id, TaskEventKind::Started { descriptor });
        self.push_task_event(
            task_id,
            TaskEventKind::SideEffectPlanned {
                effect: TaskSideEffect {
                    effect_id: tool_call_id.to_string(),
                    kind: TaskSideEffectKind::Execution,
                    summary: format!("tool call started: {label}"),
                    native_refs: Vec::new(),
                    payload: Some(json!({
                        "tool_call_id": tool_call_id,
                        "name": name,
                        "event_id": envelope.event_id,
                    })),
                },
            },
        );
        self.invalidate_main_views("tool call started");
    }

    fn complete_tool_call(
        &mut self,
        envelope: &EventEnvelope,
        tool_call_id: &str,
        name: Option<&str>,
        success: Option<bool>,
        result_preview: Option<&str>,
        presentation: Option<&ToolResultPresentation>,
    ) {
        let task_id = self.tool_tasks.get(tool_call_id).copied();
        let title = name.unwrap_or("tool");
        let summary = result_preview
            .map(str::to_string)
            .or_else(|| presentation.map(tool_presentation_summary))
            .unwrap_or_else(|| title.to_string());
        self.conversation_blocks.push(ConversationBlock {
            id: format!("tool:{tool_call_id}:{}", envelope.sequence),
            kind: ConversationBlockKind::Tool,
            text: summary.clone(),
            task_id,
            artifact_id: None,
            evidence_ids: Vec::new(),
        });
        let evidence_id = if result_preview.is_some() || presentation.is_some() {
            Some(self.record_evidence(
                envelope,
                format!("Tool result: {title}"),
                json!({
                    "tool_call_id": tool_call_id,
                    "name": title,
                    "success": success,
                    "preview": result_preview,
                    "presentation": presentation,
                }),
                task_id,
            ))
        } else {
            None
        };
        if let Some(task_id) = task_id {
            if let Some(task) = self.tasks.get_mut(&task_id) {
                task.status = if success == Some(false) {
                    "failed".to_string()
                } else {
                    "completed".to_string()
                };
            }
            if success == Some(false) {
                self.push_task_event(
                    task_id,
                    TaskEventKind::Failed {
                        failure: TaskFailure {
                            code: "tool_failed".to_string(),
                            message: summary,
                            retryable: true,
                            evidence_ids: evidence_id.into_iter().collect(),
                        },
                    },
                );
            } else {
                self.push_task_event(
                    task_id,
                    TaskEventKind::Completed {
                        summary: Some(summary),
                        artifact_ids: Vec::new(),
                        evidence_ids: evidence_id.into_iter().collect(),
                    },
                );
            }
        }
        self.invalidate_main_views("tool call completed");
    }

    fn record_plan(
        &mut self,
        envelope: &EventEnvelope,
        explanation: Option<&str>,
        items: &[alan_protocol::PlanItem],
    ) {
        let artifact_id = ArtifactId::new();
        let title = explanation.unwrap_or("Plan updated");
        let mut metadata = DescriptorMetadata::new(title);
        metadata.tags = vec!["alan-agent".to_string(), "plan".to_string()];
        metadata.attributes.insert(
            "items".to_string(),
            json!(
                items
                    .iter()
                    .map(|item| json!({
                        "id": item.id,
                        "content": item.content,
                        "status": plan_status_label(&item.status),
                    }))
                    .collect::<Vec<_>>()
            ),
        );
        let descriptor = ArtifactDescriptor {
            id: artifact_id,
            task_id: self.active_turn_task,
            object_id: Some(self.ids.objects.plans),
            buffer_id: Some(self.ids.surfaces.conversation_buffer),
            native_ref: None,
            metadata,
        };
        self.artifact_items.push(ObjectListItem {
            object_id: self.ids.objects.plans,
            title: title.to_string(),
            subtitle: Some(format!("{} plan items", items.len())),
        });
        self.conversation_blocks.push(ConversationBlock {
            id: format!("plan:{}", envelope.sequence),
            kind: ConversationBlockKind::Artifact,
            text: plan_summary(explanation, items),
            task_id: self.active_turn_task,
            artifact_id: Some(artifact_id),
            evidence_ids: Vec::new(),
        });
        self.push_event(KernelEventKind::ArtifactRecorded {
            descriptor: descriptor.clone(),
        });
        if let Some(task_id) = self.active_turn_task {
            self.push_task_event(
                task_id,
                TaskEventKind::ArtifactCreated {
                    artifact: descriptor,
                },
            );
        }
        self.invalidate_main_views("plan updated");
    }

    fn record_yield(
        &mut self,
        envelope: &EventEnvelope,
        request_id: &str,
        kind: &YieldKind,
        payload: &Value,
    ) {
        let task_id = self.ensure_active_task("Pending agent input");
        if let Some(task) = self.tasks.get_mut(&task_id) {
            task.status = "yielded".to_string();
        }
        let yield_kind = task_yield_kind(kind);
        self.pending_form = Some(form_for_yield(
            request_id,
            kind,
            payload,
            &self.ids.commands,
        ));
        self.conversation_blocks.push(ConversationBlock {
            id: format!("yield:{request_id}"),
            kind: ConversationBlockKind::Yield,
            text: yield_title(kind, payload),
            task_id: Some(task_id),
            artifact_id: None,
            evidence_ids: Vec::new(),
        });
        self.push_task_event(
            task_id,
            TaskEventKind::Yielded {
                checkpoint: TaskYieldCheckpoint {
                    request_id: request_id.to_string(),
                    kind: yield_kind,
                    payload: payload.clone(),
                    resumable: true,
                },
            },
        );
        self.record_evidence(
            envelope,
            format!("Yield checkpoint: {request_id}"),
            json!({
                "request_id": request_id,
                "kind": kind,
                "payload": payload,
            }),
            Some(task_id),
        );
        self.invalidate_view(
            self.ids.surfaces.approval_form_view,
            "yield checkpoint updated",
        );
        self.invalidate_main_views("yield checkpoint updated");
    }

    fn record_memory_flush(
        &mut self,
        envelope: &EventEnvelope,
        attempt: &alan_protocol::MemoryFlushAttemptSnapshot,
    ) {
        let title = match attempt.result {
            MemoryFlushResult::Success => "Memory flush succeeded",
            MemoryFlushResult::Skipped => "Memory flush skipped",
            MemoryFlushResult::Failure => "Memory flush failed",
        };
        self.memory_items.push(ObjectListItem {
            object_id: self.ids.objects.memory_entries,
            title: title.to_string(),
            subtitle: attempt
                .output_path
                .clone()
                .or_else(|| attempt.error_message.clone()),
        });
        let native_ref = attempt.output_path.as_ref().map(|path| {
            NativeReference::File(FileReference {
                path: path.clone(),
                version: Some(attempt.attempt_id.clone()),
            })
        });
        self.record_evidence_with_native_ref(
            envelope,
            title,
            json!({
                "kind": "memory_flush",
                "attempt_id": attempt.attempt_id,
                "result": attempt.result,
                "skip_reason": attempt.skip_reason,
                "source_messages": attempt.source_messages,
                "warning_message": attempt.warning_message,
                "error_message": attempt.error_message,
                "timestamp": attempt.timestamp,
            }),
            self.active_turn_task,
            native_ref,
        );
        self.progress_current_task("memory flush observed", Some("memory".into()));
        self.invalidate_view(
            self.ids.surfaces.memory_review_view,
            "memory review updated",
        );
    }

    fn project_child_run_event(&mut self, child_run: AgentWorkspaceChildRunEvent) {
        let task_id = self
            .tool_tasks
            .get(&child_run.child_run_id)
            .copied()
            .unwrap_or_else(|| self.ensure_child_run_task(&child_run));
        for evidence in child_run.evidence {
            self.record_evidence_input(evidence, Some(task_id));
        }
        match child_run.status {
            AgentWorkspaceChildRunStatus::Started => {}
            AgentWorkspaceChildRunStatus::Running => self.push_task_event(
                task_id,
                TaskEventKind::Progress {
                    progress: TaskProgress {
                        label: Some("child_run".to_string()),
                        completed: None,
                        total: None,
                        fraction: None,
                        message: child_run.summary.clone(),
                    },
                },
            ),
            AgentWorkspaceChildRunStatus::Yielded => {
                self.update_task_entry_status(task_id, "yielded");
                self.push_task_event(
                    task_id,
                    TaskEventKind::Yielded {
                        checkpoint: TaskYieldCheckpoint {
                            request_id: child_run.child_run_id.clone(),
                            kind: TaskYieldKind::Other("child_run".to_string()),
                            payload: json!({
                                "summary": child_run.summary,
                                "delegated_skill": child_run.delegated_skill,
                            }),
                            resumable: true,
                        },
                    },
                );
            }
            AgentWorkspaceChildRunStatus::Completed => {
                self.update_task_entry_status(task_id, "completed");
                self.push_task_event(
                    task_id,
                    TaskEventKind::Completed {
                        summary: child_run.summary,
                        artifact_ids: Vec::new(),
                        evidence_ids: Vec::new(),
                    },
                );
            }
            AgentWorkspaceChildRunStatus::Failed => {
                self.update_task_entry_status(task_id, "failed");
                self.push_task_event(
                    task_id,
                    TaskEventKind::Failed {
                        failure: TaskFailure {
                            code: "child_run_failed".to_string(),
                            message: child_run
                                .summary
                                .unwrap_or_else(|| "child run failed".to_string()),
                            retryable: true,
                            evidence_ids: Vec::new(),
                        },
                    },
                );
            }
            AgentWorkspaceChildRunStatus::Cancelled => {
                self.update_task_entry_status(task_id, "cancelled");
                self.push_task_event(
                    task_id,
                    TaskEventKind::Cancelled {
                        reason: child_run.summary,
                    },
                );
            }
        }
        self.invalidate_main_views("child run updated");
    }

    fn ensure_child_run_task(&mut self, child_run: &AgentWorkspaceChildRunEvent) -> TaskId {
        let task_id = TaskId::new();
        let title = child_run
            .summary
            .clone()
            .or_else(|| child_run.delegated_skill.clone())
            .unwrap_or_else(|| format!("Child run {}", child_run.child_run_id));
        let descriptor = TaskDescriptor {
            id: task_id,
            actor_id: self.ids.agent_actor,
            parent_task_id: self.active_turn_task,
            command_id: None,
            status: match child_run.status {
                AgentWorkspaceChildRunStatus::Yielded => TaskStatus::Yielded,
                AgentWorkspaceChildRunStatus::Completed => TaskStatus::Completed,
                AgentWorkspaceChildRunStatus::Failed => TaskStatus::Failed,
                AgentWorkspaceChildRunStatus::Cancelled => TaskStatus::Cancelled,
                AgentWorkspaceChildRunStatus::Started | AgentWorkspaceChildRunStatus::Running => {
                    TaskStatus::Running
                }
            },
            metadata: DescriptorMetadata::new(title),
        };
        self.tool_tasks
            .insert(child_run.child_run_id.clone(), task_id);
        self.tasks.insert(
            task_id,
            AgentWorkspaceTaskEntry::from_descriptor(&descriptor, self.active_turn_task),
        );
        self.push_task_event(task_id, TaskEventKind::Started { descriptor });
        task_id
    }

    fn project_memory_observation(&mut self, observation: AgentWorkspaceMemoryObservation) {
        let kind_label = match observation.kind {
            AgentWorkspaceMemoryObservationKind::Recall => "memory_recall",
            AgentWorkspaceMemoryObservationKind::Promotion => "memory_promotion",
            AgentWorkspaceMemoryObservationKind::Flush => "memory_flush",
        };
        self.memory_items.push(ObjectListItem {
            object_id: self.ids.objects.memory_entries,
            title: observation.title.clone(),
            subtitle: observation.preview.clone(),
        });
        self.record_evidence_with_native_ref(
            &synthetic_envelope(self.sequence + 1),
            observation.title,
            json!({
                "kind": kind_label,
                "preview": observation.preview,
                "payload": observation.payload,
            }),
            self.active_turn_task,
            observation.native_ref,
        );
        self.invalidate_view(
            self.ids.surfaces.memory_review_view,
            "memory review updated",
        );
        self.invalidate_view(self.ids.surfaces.evidence_view, "memory evidence updated");
    }

    fn project_rollout_record(&mut self, record: AgentWorkspaceRolloutRecord) {
        match record {
            AgentWorkspaceRolloutRecord::Artifact {
                title,
                native_ref,
                payload,
            } => self.project_rollout_artifact(title, native_ref, payload),
            AgentWorkspaceRolloutRecord::Effect {
                effect_id,
                kind,
                summary,
                committed,
                native_refs,
                payload,
            } => self.project_rollout_effect(
                effect_id,
                kind,
                summary,
                committed,
                native_refs,
                payload,
            ),
            AgentWorkspaceRolloutRecord::Checkpoint {
                checkpoint_id,
                title,
                payload,
            } => self.project_rollout_checkpoint(checkpoint_id, title, payload),
            AgentWorkspaceRolloutRecord::Evidence(evidence) => {
                self.record_evidence_input(evidence, self.active_turn_task);
            }
        }
        self.invalidate_main_views("rollout record updated");
    }

    fn project_rollout_artifact(
        &mut self,
        title: String,
        native_ref: Option<NativeReference>,
        payload: Value,
    ) {
        let artifact_id = ArtifactId::new();
        let mut metadata = DescriptorMetadata::new(title.clone());
        metadata.tags = vec!["alan-agent".to_string(), "rollout_artifact".to_string()];
        metadata.attributes.insert("payload".to_string(), payload);
        let descriptor = ArtifactDescriptor {
            id: artifact_id,
            task_id: self.active_turn_task,
            object_id: Some(self.ids.objects.artifacts),
            buffer_id: Some(self.ids.surfaces.evidence_buffer),
            native_ref,
            metadata,
        };
        self.artifact_items.push(ObjectListItem {
            object_id: self.ids.objects.artifacts,
            title,
            subtitle: Some("rollout artifact".to_string()),
        });
        self.push_event(KernelEventKind::ArtifactRecorded {
            descriptor: descriptor.clone(),
        });
        if let Some(task_id) = self.active_turn_task {
            self.push_task_event(
                task_id,
                TaskEventKind::ArtifactCreated {
                    artifact: descriptor,
                },
            );
        }
    }

    fn project_rollout_effect(
        &mut self,
        effect_id: String,
        kind: AgentWorkspaceEffectKind,
        summary: String,
        committed: bool,
        native_refs: Vec<NativeReference>,
        payload: Option<Value>,
    ) {
        let Some(task_id) = self.active_turn_task else {
            self.record_evidence_input(
                AgentWorkspaceEvidenceInput {
                    title: summary,
                    native_ref: None,
                    payload: json!({
                        "effect_id": effect_id,
                        "kind": kind,
                        "committed": committed,
                        "payload": payload,
                    }),
                },
                None,
            );
            return;
        };
        let effect = TaskSideEffect {
            effect_id,
            kind: task_side_effect_kind(kind),
            summary,
            native_refs,
            payload,
        };
        if committed {
            self.push_task_event(
                task_id,
                TaskEventKind::SideEffectCommitted {
                    effect,
                    evidence_ids: Vec::new(),
                },
            );
        } else {
            self.push_task_event(task_id, TaskEventKind::SideEffectPlanned { effect });
        }
    }

    fn project_rollout_checkpoint(&mut self, checkpoint_id: String, title: String, payload: Value) {
        let task_id = self.active_turn_task;
        self.record_evidence_input(
            AgentWorkspaceEvidenceInput {
                title: title.clone(),
                native_ref: None,
                payload: json!({
                    "checkpoint_id": checkpoint_id,
                    "payload": payload,
                }),
            },
            task_id,
        );
        if let Some(task_id) = task_id {
            self.push_task_event(
                task_id,
                TaskEventKind::Yielded {
                    checkpoint: TaskYieldCheckpoint {
                        request_id: checkpoint_id,
                        kind: TaskYieldKind::Other("rollout_checkpoint".to_string()),
                        payload: json!({"title": title}),
                        resumable: true,
                    },
                },
            );
        }
    }

    fn record_error(&mut self, envelope: &EventEnvelope, message: &str, recoverable: bool) {
        self.conversation_blocks.push(ConversationBlock {
            id: format!("error:{}", envelope.sequence),
            kind: ConversationBlockKind::Error,
            text: message.to_string(),
            task_id: self.active_turn_task,
            artifact_id: None,
            evidence_ids: Vec::new(),
        });
        if let Some(task_id) = self.active_turn_task {
            if recoverable {
                self.progress_current_task(
                    format!("recoverable error: {message}"),
                    Some("error".into()),
                );
            } else {
                if let Some(task) = self.tasks.get_mut(&task_id) {
                    task.status = "failed".to_string();
                }
                self.push_task_event(
                    task_id,
                    TaskEventKind::Failed {
                        failure: TaskFailure {
                            code: "runtime_error".to_string(),
                            message: message.to_string(),
                            retryable: false,
                            evidence_ids: Vec::new(),
                        },
                    },
                );
            }
        }
        self.invalidate_main_views("error observed");
    }

    fn append_conversation_block(
        &mut self,
        id: String,
        kind: ConversationBlockKind,
        chunk: &str,
        merge_same_block: bool,
    ) {
        if merge_same_block
            && let Some(existing) = self
                .conversation_blocks
                .last_mut()
                .filter(|block| block.id == id && block.kind == kind)
        {
            existing.text.push_str(chunk);
            return;
        }
        self.conversation_blocks.push(ConversationBlock {
            id,
            kind,
            text: chunk.to_string(),
            task_id: self.active_turn_task,
            artifact_id: None,
            evidence_ids: Vec::new(),
        });
    }

    fn append_task_output(&mut self, stream: TaskOutputStream, content: String, terminal: bool) {
        if let Some(task_id) = self.active_turn_task {
            self.push_task_event(
                task_id,
                TaskEventKind::OutputAppended {
                    output: TaskOutputChunk {
                        stream,
                        content,
                        terminal,
                    },
                },
            );
        }
    }

    fn ensure_active_task(&mut self, title: &str) -> TaskId {
        if let Some(task_id) = self.active_turn_task {
            return task_id;
        }
        let task_id = TaskId::new();
        self.active_turn_task = Some(task_id);
        let descriptor = TaskDescriptor {
            id: task_id,
            actor_id: self.ids.agent_actor,
            parent_task_id: None,
            command_id: None,
            status: TaskStatus::Running,
            metadata: DescriptorMetadata::new(title),
        };
        self.tasks.insert(
            task_id,
            AgentWorkspaceTaskEntry::from_descriptor(&descriptor, None),
        );
        self.push_task_event(task_id, TaskEventKind::Started { descriptor });
        task_id
    }

    fn progress_current_task(&mut self, message: impl Into<String>, label: Option<String>) {
        if let Some(task_id) = self.active_turn_task {
            self.push_task_event(
                task_id,
                TaskEventKind::Progress {
                    progress: TaskProgress {
                        label,
                        completed: None,
                        total: None,
                        fraction: None,
                        message: Some(message.into()),
                    },
                },
            );
        }
    }

    fn record_evidence(
        &mut self,
        envelope: &EventEnvelope,
        title: impl Into<String>,
        payload: Value,
        task_id: Option<TaskId>,
    ) -> EvidenceId {
        self.record_evidence_with_native_ref(envelope, title, payload, task_id, None)
    }

    fn record_evidence_with_native_ref(
        &mut self,
        envelope: &EventEnvelope,
        title: impl Into<String>,
        payload: Value,
        task_id: Option<TaskId>,
        native_ref: Option<NativeReference>,
    ) -> EvidenceId {
        let title = title.into();
        let evidence_id = EvidenceId::new();
        let mut metadata = DescriptorMetadata::new(title.clone());
        metadata.tags = vec!["alan-agent".to_string(), "evidence".to_string()];
        metadata.attributes.insert("payload".to_string(), payload);
        metadata
            .attributes
            .insert("protocol_event_id".to_string(), json!(envelope.event_id));
        let descriptor = EvidenceDescriptor {
            id: evidence_id,
            task_id,
            artifact_id: None,
            event_id: None,
            native_ref,
            metadata,
        };
        self.evidence_items.push(ObjectListItem {
            object_id: self.ids.objects.evidence,
            title,
            subtitle: Some(format!("event {}", envelope.event_id)),
        });
        self.push_event(KernelEventKind::EvidenceRecorded {
            descriptor: descriptor.clone(),
        });
        if let Some(task_id) = task_id {
            self.push_task_event(
                task_id,
                TaskEventKind::EvidenceAttached {
                    evidence: descriptor,
                },
            );
        }
        evidence_id
    }

    fn record_evidence_input(
        &mut self,
        evidence: AgentWorkspaceEvidenceInput,
        task_id: Option<TaskId>,
    ) -> EvidenceId {
        self.record_evidence_with_native_ref(
            &synthetic_envelope(self.sequence + 1),
            evidence.title,
            evidence.payload,
            task_id,
            evidence.native_ref,
        )
    }

    fn push_task_event(&mut self, task_id: TaskId, kind: TaskEventKind) {
        self.push_event(KernelEventKind::Task {
            event: TaskEvent { task_id, kind },
        });
    }

    fn invalidate_main_views(&mut self, reason: &str) {
        self.invalidate_view(self.ids.surfaces.conversation_view, reason);
        self.invalidate_view(self.ids.surfaces.task_tree_view, reason);
        self.invalidate_view(self.ids.surfaces.evidence_view, reason);
        self.invalidate_view(self.ids.surfaces.audit_view, reason);
    }

    fn invalidate_view(&mut self, view_id: ViewId, reason: &str) {
        self.push_event(KernelEventKind::ViewInvalidated {
            view_id,
            reason: Some(reason.to_string()),
        });
    }

    fn push_event(&mut self, kind: KernelEventKind) {
        self.sequence += 1;
        let event_id = EventId::new();
        self.kernel_events.push(KernelEvent::root(
            event_id,
            self.sequence,
            0,
            self.ids.system_actor,
            kind,
        ));
    }

    fn audit(&mut self, timestamp_ms: u64, label: impl Into<String>) {
        self.audit_entries.push(LogEntry {
            timestamp_ms,
            level: "info".to_string(),
            message: label.into(),
        });
    }

    fn update_task_entry_status(&mut self, task_id: TaskId, status: &str) {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            task.status = status.to_string();
        }
    }

    fn session_native_ref(&self) -> NativeReference {
        NativeReference::AgentSession(AgentSessionReference {
            adapter: COMPATIBILITY_SESSION_ADAPTER.to_string(),
            session_id: self.session.session_id.clone(),
        })
    }

    fn conversation_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.conversation_view,
            self.ids.surfaces.conversation_buffer,
            ViewKind::Conversation,
            ViewModel::Conversation(alan_kernel::ConversationViewModel {
                blocks: self.conversation_blocks.clone(),
            }),
            vec![
                action(self.ids.commands.submit_turn, "Submit Turn", true),
                action(
                    self.ids.commands.interrupt,
                    "Interrupt",
                    self.active_turn_task.is_some(),
                ),
            ],
        )
    }

    fn task_tree_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.task_tree_view,
            self.ids.surfaces.task_tree_buffer,
            ViewKind::TaskTree,
            ViewModel::TaskTree(TaskTreeViewModel {
                roots: self.task_roots(),
            }),
            vec![
                action(
                    self.ids.commands.interrupt,
                    "Interrupt",
                    self.active_turn_task.is_some(),
                ),
                action(self.ids.commands.rollback, "Rollback", true),
            ],
        )
    }

    fn evidence_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.evidence_view,
            self.ids.surfaces.evidence_buffer,
            ViewKind::ObjectList,
            ViewModel::ObjectList(ObjectListViewModel {
                objects: self.evidence_items.clone(),
            }),
            vec![action(
                self.ids.commands.inspect_evidence,
                "Inspect Evidence",
                !self.evidence_items.is_empty(),
            )],
        )
    }

    fn memory_review_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.memory_review_view,
            self.ids.surfaces.memory_review_buffer,
            ViewKind::ObjectList,
            ViewModel::ObjectList(ObjectListViewModel {
                objects: self.memory_items.clone(),
            }),
            vec![action(
                self.ids.commands.open_memory_review,
                "Open Memory Review",
                true,
            )],
        )
    }

    fn approval_form_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.approval_form_view,
            self.ids.surfaces.approval_form_buffer,
            ViewKind::Form,
            ViewModel::Form(self.pending_form.clone().unwrap_or_else(|| FormViewModel {
                title: "Approval".to_string(),
                fields: Vec::new(),
                submit_command_id: Some(self.ids.commands.resume_yield),
            })),
            vec![
                action(
                    self.ids.commands.resume_yield,
                    "Resume Yield",
                    self.pending_form.is_some(),
                ),
                action(
                    self.ids.commands.approve_command,
                    "Approve",
                    self.pending_form.is_some(),
                ),
                action(
                    self.ids.commands.deny_command,
                    "Deny",
                    self.pending_form.is_some(),
                ),
            ],
        )
    }

    fn command_palette_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.command_palette_view,
            self.ids.surfaces.command_palette_buffer,
            ViewKind::CommandPalette,
            ViewModel::CommandPalette(alan_kernel::CommandPaletteViewModel {
                query: String::new(),
                entries: self
                    .command_descriptors()
                    .into_iter()
                    .map(|command| alan_kernel::CommandPaletteEntry {
                        command_id: command.id,
                        title: command.metadata.title,
                        subtitle: Some(command.name),
                        enabled: true,
                    })
                    .collect(),
            }),
            Vec::new(),
        )
    }

    fn audit_snapshot(&self) -> ViewSnapshot {
        self.snapshot(
            self.ids.surfaces.audit_view,
            self.ids.surfaces.audit_buffer,
            ViewKind::LogStream,
            ViewModel::LogStream(LogStreamViewModel {
                entries: self.audit_entries.clone(),
            }),
            Vec::new(),
        )
    }

    fn snapshot(
        &self,
        view_id: ViewId,
        buffer_id: BufferId,
        kind: ViewKind,
        model: ViewModel,
        actions: Vec<ViewAction>,
    ) -> ViewSnapshot {
        ViewSnapshot {
            view_id,
            buffer_id,
            version: self.snapshot_version,
            kind,
            model,
            actions,
            diagnostics: Vec::new(),
            selection: None,
            focus: None,
            semantic_state: ViewSemanticState::default(),
        }
    }

    fn task_roots(&self) -> Vec<TaskTreeNode> {
        self.tasks
            .iter()
            .filter(|(_, entry)| entry.parent_task_id.is_none())
            .map(|(task_id, _)| self.task_node(*task_id))
            .collect()
    }

    fn task_node(&self, task_id: TaskId) -> TaskTreeNode {
        let entry = self.tasks.get(&task_id).expect("task entry exists");
        TaskTreeNode {
            task_id,
            label: entry.label.clone(),
            status: entry.status.clone(),
            children: self
                .tasks
                .iter()
                .filter(|(_, child)| child.parent_task_id == Some(task_id))
                .map(|(child_task_id, _)| self.task_node(*child_task_id))
                .collect(),
        }
    }
}

/// Semantic snapshots produced by the Agent Workspace projection.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentWorkspaceSnapshots {
    /// Conversation snapshot.
    pub conversation: ViewSnapshot,
    /// Task tree snapshot.
    pub task_tree: ViewSnapshot,
    /// Evidence browser snapshot.
    pub evidence: ViewSnapshot,
    /// Memory review snapshot.
    pub memory_review: ViewSnapshot,
    /// Approval form snapshot.
    pub approval_form: ViewSnapshot,
    /// Command palette snapshot.
    pub command_palette: ViewSnapshot,
    /// Audit log snapshot.
    pub audit: ViewSnapshot,
}

#[derive(Clone, Debug)]
struct AgentWorkspaceTaskEntry {
    label: String,
    status: String,
    parent_task_id: Option<TaskId>,
}

impl AgentWorkspaceTaskEntry {
    fn from_descriptor(descriptor: &TaskDescriptor, parent_task_id: Option<TaskId>) -> Self {
        Self {
            label: descriptor.metadata.title.clone(),
            status: task_status_label(&descriptor.status).to_string(),
            parent_task_id,
        }
    }
}

fn action(command_id: CommandId, label: &str, enabled: bool) -> ViewAction {
    ViewAction {
        command_id,
        label: label.to_string(),
        enabled,
    }
}

fn session_attributes(session: &AgentWorkspaceSessionMetadata) -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("app_id".to_string(), json!(ALAN_AGENT_APP_ID)),
        ("session_id".to_string(), json!(session.session_id)),
        ("workspace_dir".to_string(), json!(session.workspace_dir)),
        ("agent_name".to_string(), json!(session.agent_name)),
        ("profile_id".to_string(), json!(session.profile_id)),
        ("provider".to_string(), json!(session.provider)),
        ("resolved_model".to_string(), json!(session.resolved_model)),
    ])
}

fn task_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Yielded => "yielded",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn plan_status_label(status: &PlanItemStatus) -> &'static str {
    match status {
        PlanItemStatus::Pending => "pending",
        PlanItemStatus::InProgress => "in_progress",
        PlanItemStatus::Completed => "completed",
    }
}

fn plan_summary(explanation: Option<&str>, items: &[alan_protocol::PlanItem]) -> String {
    let mut lines = Vec::new();
    if let Some(explanation) = explanation {
        lines.push(explanation.to_string());
    }
    lines.extend(
        items
            .iter()
            .map(|item| format!("[{}] {}", plan_status_label(&item.status), item.content)),
    );
    lines.join("\n")
}

fn task_yield_kind(kind: &YieldKind) -> TaskYieldKind {
    match kind {
        YieldKind::Confirmation => TaskYieldKind::Confirmation,
        YieldKind::StructuredInput => TaskYieldKind::StructuredInput,
        YieldKind::DynamicTool => TaskYieldKind::DynamicTool,
        YieldKind::Custom(value) => TaskYieldKind::Other(value.clone()),
    }
}

fn task_side_effect_kind(kind: AgentWorkspaceEffectKind) -> TaskSideEffectKind {
    match kind {
        AgentWorkspaceEffectKind::KernelState => TaskSideEffectKind::KernelState,
        AgentWorkspaceEffectKind::FileSystem => TaskSideEffectKind::FileSystem,
        AgentWorkspaceEffectKind::Execution => TaskSideEffectKind::Execution,
        AgentWorkspaceEffectKind::Network => TaskSideEffectKind::Network,
        AgentWorkspaceEffectKind::Terminal => TaskSideEffectKind::Terminal,
        AgentWorkspaceEffectKind::Other(value) => TaskSideEffectKind::Other(value),
    }
}

fn yield_title(kind: &YieldKind, payload: &Value) -> String {
    payload
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| payload.get("title").and_then(Value::as_str))
        .map(str::to_string)
        .unwrap_or_else(|| match kind {
            YieldKind::Confirmation => "Confirmation requested".to_string(),
            YieldKind::StructuredInput => "Structured input requested".to_string(),
            YieldKind::DynamicTool => "Client tool requested".to_string(),
            YieldKind::Custom(kind) => format!("{kind} requested"),
        })
}

fn form_for_yield(
    request_id: &str,
    kind: &YieldKind,
    payload: &Value,
    commands: &AgentWorkspaceCommandIds,
) -> FormViewModel {
    let fields = match kind {
        YieldKind::Confirmation => vec![FormField {
            id: "choice".to_string(),
            label: "Choice".to_string(),
            kind: FormFieldKind::Select,
            value: json!(
                payload
                    .get("default_option")
                    .and_then(Value::as_str)
                    .unwrap_or("approve")
            ),
            required: true,
        }],
        YieldKind::StructuredInput => structured_input_fields(payload),
        YieldKind::DynamicTool | YieldKind::Custom(_) => vec![FormField {
            id: "payload".to_string(),
            label: "Payload".to_string(),
            kind: FormFieldKind::Json,
            value: payload.clone(),
            required: true,
        }],
    };
    FormViewModel {
        title: format!("Yield {request_id}"),
        fields,
        submit_command_id: Some(commands.resume_yield),
    }
}

fn structured_input_fields(payload: &Value) -> Vec<FormField> {
    payload
        .get("questions")
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .map(|question| {
                    let id = question
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("answer")
                        .to_string();
                    let label = question
                        .get("label")
                        .and_then(Value::as_str)
                        .or_else(|| question.get("prompt").and_then(Value::as_str))
                        .unwrap_or(id.as_str())
                        .to_string();
                    FormField {
                        id,
                        label,
                        kind: FormFieldKind::Json,
                        value: question
                            .get("default_value")
                            .cloned()
                            .unwrap_or(Value::Null),
                        required: question
                            .get("required")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    }
                })
                .collect()
        })
        .unwrap_or_else(|| {
            vec![FormField {
                id: "answer".to_string(),
                label: "Answer".to_string(),
                kind: FormFieldKind::TextArea,
                value: Value::Null,
                required: true,
            }]
        })
}

fn protocol_event_label(event: &Event) -> &'static str {
    match event {
        Event::TurnStarted { .. } => "turn started",
        Event::TurnCompleted { .. } => "turn completed",
        Event::TextDelta { .. } => "text delta",
        Event::ThinkingDelta { .. } => "thinking delta",
        Event::ToolCallStarted { .. } => "tool call started",
        Event::ToolCallCompleted { .. } => "tool call completed",
        Event::PlanUpdated { .. } => "plan updated",
        Event::SessionRolledBack { .. } => "session rolled back",
        Event::Yield { .. } => "yield",
        Event::CompactionObserved { .. } => "compaction observed",
        Event::MemoryFlushObserved { .. } => "memory flush observed",
        Event::Warning { .. } => "warning",
        Event::Error { .. } => "error",
    }
}

fn synthetic_envelope(sequence: u64) -> EventEnvelope {
    EventEnvelope {
        event_id: format!("agent-workspace-{sequence}"),
        sequence,
        session_id: "agent-workspace".to_string(),
        submission_id: None,
        turn_id: "agent-workspace".to_string(),
        item_id: format!("item-{sequence}"),
        timestamp_ms: 0,
        event: Event::Warning {
            message: "synthetic projection input".to_string(),
        },
    }
}

fn tool_presentation_summary(presentation: &ToolResultPresentation) -> String {
    match presentation {
        ToolResultPresentation::Diff { path, .. } => format!("diff for {path}"),
        ToolResultPresentation::FileContent {
            path,
            lines,
            truncated,
        } => {
            let suffix = if *truncated { ", truncated" } else { "" };
            format!("file {path} ({lines} lines{suffix})")
        }
        ToolResultPresentation::Command {
            cmdline,
            exit_code,
            truncated,
            ..
        } => {
            let code = exit_code
                .map(|code| format!(" exit {code}"))
                .unwrap_or_default();
            let suffix = if *truncated { " truncated" } else { "" };
            format!("command `{cmdline}`{code}{suffix}")
        }
        ToolResultPresentation::Listing { rows } => format!("listing with {} rows", rows.len()),
        ToolResultPresentation::PlainText { body } => body.lines().next().unwrap_or("").to_string(),
    }
}
