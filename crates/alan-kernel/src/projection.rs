use crate::{
    ArtifactDescriptor, ArtifactId, BufferDescriptor, BufferId, CommandId, CommandTarget,
    EvidenceDescriptor, EvidenceId, KernelEvent, KernelEventKind, ObjectDescriptor, ObjectId,
    SubscriptionDependency, SubscriptionMessageKind, TaskDescriptor, TaskEvent, TaskEventKind,
    TaskId, TaskStatus, ViewDescriptor, ViewId,
};
use std::collections::BTreeMap;

/// Command availability projection for a semantic target.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandAvailabilityProjection {
    /// Target whose command availability was computed.
    pub target: CommandTarget,
    /// Commands available for the target.
    pub command_ids: Vec<CommandId>,
}

/// Dirty semantic view marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirtyView {
    /// Dirty view id.
    pub view_id: ViewId,
    /// Event that dirtied the view, if known.
    pub event_id: Option<crate::EventId>,
    /// Optional invalidation reason.
    pub reason: Option<String>,
}

/// Rebuildable in-memory projection cache for Alan Kernel semantic state.
#[derive(Clone, Debug, Default)]
pub struct ProjectionStore {
    objects: BTreeMap<ObjectId, ObjectDescriptor>,
    buffers: BTreeMap<BufferId, BufferDescriptor>,
    views: BTreeMap<ViewId, ViewDescriptor>,
    tasks: BTreeMap<TaskId, TaskDescriptor>,
    artifacts: BTreeMap<ArtifactId, ArtifactDescriptor>,
    evidence: BTreeMap<EvidenceId, EvidenceDescriptor>,
    command_availability: Vec<CommandAvailabilityProjection>,
    dirty_views: BTreeMap<ViewId, DirtyView>,
}

impl ProjectionStore {
    /// Creates an empty projection store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds a projection store from replayed Kernel events.
    #[must_use]
    pub fn rebuild<'a>(events: impl IntoIterator<Item = &'a KernelEvent>) -> Self {
        let mut store = Self::new();
        for event in events {
            store.apply_event(event);
        }
        store
    }

    /// Applies one Kernel event to the projection cache.
    pub fn apply_event(&mut self, event: &KernelEvent) {
        match &event.kind {
            KernelEventKind::CommandInvoked { .. } | KernelEventKind::QueryInvoked { .. } => {}
            KernelEventKind::SubscriptionObserved { message } => {
                self.apply_subscription_message(event, &message.kind);
            }
            KernelEventKind::ObjectUpserted { descriptor } => {
                self.objects.insert(descriptor.id, descriptor.clone());
            }
            KernelEventKind::BufferUpserted { descriptor } => {
                self.buffers.insert(descriptor.id, descriptor.clone());
            }
            KernelEventKind::ViewUpserted { descriptor } => {
                self.views.insert(descriptor.id, descriptor.clone());
            }
            KernelEventKind::ArtifactRecorded { descriptor } => {
                self.artifacts.insert(descriptor.id, descriptor.clone());
            }
            KernelEventKind::EvidenceRecorded { descriptor } => {
                self.evidence.insert(descriptor.id, descriptor.clone());
            }
            KernelEventKind::CommandAvailabilityChanged {
                target,
                command_ids,
            } => self.set_command_availability(target.clone(), command_ids.clone()),
            KernelEventKind::ViewInvalidated { view_id, reason } => {
                self.dirty_views.insert(
                    *view_id,
                    DirtyView {
                        view_id: *view_id,
                        event_id: Some(event.event_id),
                        reason: reason.clone(),
                    },
                );
            }
            KernelEventKind::Task { event: task_event } => {
                self.apply_task_event(task_event);
            }
        }
    }

    /// Returns a projected object descriptor.
    #[must_use]
    pub fn object(&self, id: ObjectId) -> Option<&ObjectDescriptor> {
        self.objects.get(&id)
    }

    /// Returns a projected buffer descriptor.
    #[must_use]
    pub fn buffer(&self, id: BufferId) -> Option<&BufferDescriptor> {
        self.buffers.get(&id)
    }

    /// Returns a projected view descriptor.
    #[must_use]
    pub fn view(&self, id: ViewId) -> Option<&ViewDescriptor> {
        self.views.get(&id)
    }

    /// Returns a projected task descriptor.
    #[must_use]
    pub fn task(&self, id: TaskId) -> Option<&TaskDescriptor> {
        self.tasks.get(&id)
    }

    /// Returns a projected artifact descriptor.
    #[must_use]
    pub fn artifact(&self, id: ArtifactId) -> Option<&ArtifactDescriptor> {
        self.artifacts.get(&id)
    }

    /// Returns a projected evidence descriptor.
    #[must_use]
    pub fn evidence(&self, id: EvidenceId) -> Option<&EvidenceDescriptor> {
        self.evidence.get(&id)
    }

    /// Lists projected command availability entries.
    #[must_use]
    pub fn command_availability(&self) -> &[CommandAvailabilityProjection] {
        &self.command_availability
    }

    /// Returns dirty view markers.
    #[must_use]
    pub fn dirty_views(&self) -> &BTreeMap<ViewId, DirtyView> {
        &self.dirty_views
    }

    /// Clears a dirty view marker after a host refreshes the snapshot.
    pub fn clear_dirty_view(&mut self, view_id: ViewId) {
        self.dirty_views.remove(&view_id);
    }

    fn apply_task_event(&mut self, event: &TaskEvent) {
        match &event.kind {
            TaskEventKind::Started { descriptor } => {
                self.tasks.insert(descriptor.id, descriptor.clone());
            }
            TaskEventKind::Progress { .. }
            | TaskEventKind::OutputAppended { .. }
            | TaskEventKind::SideEffectPlanned { .. }
            | TaskEventKind::SideEffectCommitted { .. } => {}
            TaskEventKind::Yielded { .. } => {
                self.update_task_status(event.task_id, TaskStatus::Yielded);
            }
            TaskEventKind::Resumed { .. } => {
                self.update_task_status(event.task_id, TaskStatus::Running);
            }
            TaskEventKind::ArtifactCreated { artifact } => {
                self.artifacts.insert(artifact.id, artifact.clone());
            }
            TaskEventKind::EvidenceAttached { evidence } => {
                self.evidence.insert(evidence.id, evidence.clone());
            }
            TaskEventKind::Completed { .. } => {
                self.update_task_status(event.task_id, TaskStatus::Completed);
            }
            TaskEventKind::Failed { .. } => {
                self.update_task_status(event.task_id, TaskStatus::Failed);
            }
            TaskEventKind::Cancelled { .. } => {
                self.update_task_status(event.task_id, TaskStatus::Cancelled);
            }
        }
    }

    fn update_task_status(&mut self, task_id: TaskId, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(&task_id) {
            task.status = status;
        }
    }

    fn apply_subscription_message(
        &mut self,
        event: &KernelEvent,
        message: &SubscriptionMessageKind,
    ) {
        match message {
            SubscriptionMessageKind::Updated { .. } => {}
            SubscriptionMessageKind::Invalidated {
                dependency,
                reason,
                dirty_views,
            } => {
                for view_id in dirty_views {
                    self.dirty_views.insert(
                        *view_id,
                        DirtyView {
                            view_id: *view_id,
                            event_id: Some(event.event_id),
                            reason: reason.clone().or_else(|| dependency_reason(dependency)),
                        },
                    );
                }
            }
            SubscriptionMessageKind::CommandAvailabilityChanged {
                target,
                available_commands,
                dirty_views,
            } => {
                self.set_command_availability(target.clone(), available_commands.clone());
                for view_id in dirty_views {
                    self.dirty_views.insert(
                        *view_id,
                        DirtyView {
                            view_id: *view_id,
                            event_id: Some(event.event_id),
                            reason: Some("command availability changed".to_string()),
                        },
                    );
                }
            }
        }
    }

    fn set_command_availability(&mut self, target: CommandTarget, command_ids: Vec<CommandId>) {
        if let Some(existing) = self
            .command_availability
            .iter_mut()
            .find(|entry| entry.target == target)
        {
            existing.command_ids = command_ids;
        } else {
            self.command_availability
                .push(CommandAvailabilityProjection {
                    target,
                    command_ids,
                });
        }
    }
}

fn dependency_reason(dependency: &SubscriptionDependency) -> Option<String> {
    match dependency {
        SubscriptionDependency::Object { .. } => Some("object invalidated".to_string()),
        SubscriptionDependency::Buffer { .. } => Some("buffer invalidated".to_string()),
        SubscriptionDependency::View { .. } => Some("view invalidated".to_string()),
        SubscriptionDependency::Task { .. } => Some("task invalidated".to_string()),
        SubscriptionDependency::Query { .. } => Some("query invalidated".to_string()),
        SubscriptionDependency::CommandAvailability { .. } => {
            Some("command availability invalidated".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ProjectionStore;
    use crate::ActivityLedger;
    use crate::{
        ActorId, BufferDescriptor, BufferKind, BufferSource, CommandTarget, DescriptorMetadata,
        EventId, InMemoryActivityLedger, KernelEvent, KernelEventKind, ObjectDescriptor,
        ObjectKind, SubscriptionId, SubscriptionMessage, SubscriptionMessageKind, TaskDescriptor,
        TaskEvent, TaskEventKind, TaskSideEffect, TaskSideEffectKind, TaskStatus, ViewDescriptor,
        ViewKind,
    };

    #[test]
    fn projection_rebuilds_semantic_state_from_events() {
        let actor_id = ActorId::new();
        let object_id = crate::ObjectId::new();
        let buffer_id = crate::BufferId::new();
        let view_id = crate::ViewId::new();
        let task_id = crate::TaskId::new();
        let command_id = crate::CommandId::new();
        let events = vec![
            KernelEvent::root(
                EventId::new(),
                1,
                1,
                actor_id,
                KernelEventKind::ObjectUpserted {
                    descriptor: ObjectDescriptor {
                        id: object_id,
                        kind: ObjectKind::Synthetic,
                        metadata: DescriptorMetadata::new("Object"),
                        native_ref: None,
                        capabilities: Vec::new(),
                    },
                },
            ),
            KernelEvent::root(
                EventId::new(),
                2,
                2,
                actor_id,
                KernelEventKind::BufferUpserted {
                    descriptor: BufferDescriptor {
                        id: buffer_id,
                        kind: BufferKind::Object,
                        source: BufferSource::Object { id: object_id },
                        metadata: DescriptorMetadata::new("Buffer"),
                    },
                },
            ),
            KernelEvent::root(
                EventId::new(),
                3,
                3,
                actor_id,
                KernelEventKind::ViewUpserted {
                    descriptor: ViewDescriptor {
                        id: view_id,
                        buffer_id,
                        kind: ViewKind::Conversation,
                        metadata: DescriptorMetadata::new("View"),
                    },
                },
            ),
            KernelEvent::root(
                EventId::new(),
                4,
                4,
                actor_id,
                KernelEventKind::Task {
                    event: TaskEvent {
                        task_id,
                        kind: TaskEventKind::Started {
                            descriptor: TaskDescriptor {
                                id: task_id,
                                actor_id,
                                parent_task_id: None,
                                command_id: Some(command_id),
                                status: TaskStatus::Running,
                                metadata: DescriptorMetadata::new("Task"),
                            },
                        },
                    },
                },
            ),
            KernelEvent::root(
                EventId::new(),
                5,
                5,
                actor_id,
                KernelEventKind::CommandAvailabilityChanged {
                    target: CommandTarget::View { id: view_id },
                    command_ids: vec![command_id],
                },
            ),
            KernelEvent::root(
                EventId::new(),
                6,
                6,
                actor_id,
                KernelEventKind::SubscriptionObserved {
                    message: SubscriptionMessage {
                        subscription_id: SubscriptionId::new(),
                        sequence: 1,
                        observed_event_id: None,
                        kind: SubscriptionMessageKind::Invalidated {
                            dependency: crate::SubscriptionDependency::View { id: view_id },
                            reason: Some("changed".to_string()),
                            dirty_views: vec![view_id],
                        },
                    },
                },
            ),
        ];

        let store = ProjectionStore::rebuild(&events);

        assert!(store.object(object_id).is_some());
        assert!(store.buffer(buffer_id).is_some());
        assert!(store.view(view_id).is_some());
        assert_eq!(
            store.task(task_id).map(|task| &task.status),
            Some(&TaskStatus::Running)
        );
        assert_eq!(
            store.command_availability()[0].command_ids,
            vec![command_id]
        );
        assert!(store.dirty_views().contains_key(&view_id));
    }

    #[test]
    fn replay_rebuilds_projection_without_rerunning_side_effect_payloads() {
        let actor_id = ActorId::new();
        let task_id = crate::TaskId::new();
        let mut ledger = InMemoryActivityLedger::new();
        ledger
            .append(KernelEvent::root(
                EventId::new(),
                1,
                1,
                actor_id,
                KernelEventKind::Task {
                    event: TaskEvent {
                        task_id,
                        kind: TaskEventKind::Started {
                            descriptor: TaskDescriptor {
                                id: task_id,
                                actor_id,
                                parent_task_id: None,
                                command_id: None,
                                status: TaskStatus::Running,
                                metadata: DescriptorMetadata::new("Risky task"),
                            },
                        },
                    },
                },
            ))
            .expect("append start");

        for (sequence, kind) in [
            (2, TaskSideEffectKind::FileSystem),
            (3, TaskSideEffectKind::Execution),
            (4, TaskSideEffectKind::Network),
            (5, TaskSideEffectKind::Terminal),
            (6, TaskSideEffectKind::Other("agent_turn".to_string())),
            (7, TaskSideEffectKind::Other("import".to_string())),
        ] {
            ledger
                .append(KernelEvent::root(
                    EventId::new(),
                    sequence,
                    sequence,
                    actor_id,
                    KernelEventKind::Task {
                        event: TaskEvent {
                            task_id,
                            kind: TaskEventKind::SideEffectPlanned {
                                effect: TaskSideEffect {
                                    effect_id: format!("effect-{sequence}"),
                                    kind,
                                    summary: "payload is descriptive only".to_string(),
                                    native_refs: Vec::new(),
                                    payload: Some(serde_json::json!({
                                        "must_not_execute": true
                                    })),
                                },
                            },
                        },
                    },
                ))
                .expect("append side effect");
        }

        let replayed = ledger.replay().expect("replay");
        let store = ProjectionStore::rebuild(&replayed);

        assert_eq!(replayed.len(), 7);
        assert_eq!(
            store.task(task_id).map(|task| &task.status),
            Some(&TaskStatus::Running)
        );
        assert!(store.command_availability().is_empty());
        assert!(store.dirty_views().is_empty());
    }
}
