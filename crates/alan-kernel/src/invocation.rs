use crate::{
    ActorId, ArtifactId, BufferId, CapabilityRequirement, CommandDescriptor, CommandId,
    CommandRecoveryPolicy, CommandRisk, CommandTarget, EventId, EvidenceId, InvocationHintMetadata,
    NativeReference, ObjectId, QueryDescriptor, QueryId, QueryTarget, SubscriptionDependency,
    SubscriptionId, TaskId, ViewId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Records a request to invoke a command that may mutate Kernel state or initiate work.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandInvocation {
    /// Command being invoked.
    pub command_id: CommandId,
    /// Effective target for this invocation.
    pub target: CommandTarget,
    /// Actor requesting the command.
    pub actor_id: ActorId,
    /// Invocation arguments validated by the command descriptor schema.
    pub arguments: Value,
    /// Capability labels used to authorize this invocation.
    pub required_capabilities: Vec<CapabilityRequirement>,
    /// Effective command risk at invocation time.
    pub risk: CommandRisk,
    /// Undo or recovery policy recorded for audit and host surfaces.
    pub recovery: CommandRecoveryPolicy,
    /// Host-neutral invocation hints copied from the descriptor or narrowed by the caller.
    pub invocation_hints: InvocationHintMetadata,
    /// Event that directly caused this invocation, if known.
    pub causation_id: Option<EventId>,
    /// Event-chain id shared by related commands, tasks, artifacts, and evidence.
    pub correlation_id: Option<EventId>,
}

impl CommandInvocation {
    /// Creates an invocation from a registered command descriptor.
    #[must_use]
    pub fn from_descriptor(
        descriptor: &CommandDescriptor,
        actor_id: ActorId,
        arguments: Value,
    ) -> Self {
        Self {
            command_id: descriptor.id,
            target: descriptor.target.clone(),
            actor_id,
            arguments,
            required_capabilities: descriptor.required_capabilities.clone(),
            risk: descriptor.risk.clone(),
            recovery: descriptor.recovery.clone(),
            invocation_hints: descriptor.invocation_hints.clone(),
            causation_id: None,
            correlation_id: None,
        }
    }

    /// Adds causation and correlation event metadata.
    #[must_use]
    pub fn with_event_links(
        mut self,
        causation_id: Option<EventId>,
        correlation_id: Option<EventId>,
    ) -> Self {
        self.causation_id = causation_id;
        self.correlation_id = correlation_id;
        self
    }
}

/// Read-only semantic result references returned or expected by a query.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryResultReference {
    /// Result references an object without mutating it.
    Object { id: ObjectId },
    /// Result references a buffer without mutating it.
    Buffer { id: BufferId },
    /// Result references a view without mutating it.
    View { id: ViewId },
    /// Result references a task without mutating it.
    Task { id: TaskId },
    /// Result references an artifact without mutating it.
    Artifact { id: ArtifactId },
    /// Result references evidence without mutating it.
    Evidence { id: EvidenceId },
    /// Result references the activity event that produced or justified the data.
    Event { id: EventId },
    /// Result references a native resource through adapter-owned read authority.
    Native { native_ref: NativeReference },
    /// Result is inline JSON data owned by the query response.
    InlineJson {
        /// Optional schema id for the inline data.
        schema_id: Option<String>,
    },
}

/// Records a read-only query invocation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueryInvocation {
    /// Query being invoked.
    pub query_id: QueryId,
    /// Effective target for this read.
    pub target: QueryTarget,
    /// Actor requesting the read.
    pub actor_id: ActorId,
    /// Query parameters validated by the query descriptor schema.
    pub parameters: Value,
    /// Read-only result references requested or returned by the query.
    pub result_refs: Vec<QueryResultReference>,
    /// Capability labels used to authorize this read.
    pub required_capabilities: Vec<CapabilityRequirement>,
    /// Event that directly caused this query, if known.
    pub causation_id: Option<EventId>,
    /// Event-chain id shared by related commands, tasks, artifacts, and evidence.
    pub correlation_id: Option<EventId>,
}

impl QueryInvocation {
    /// Creates a read-only invocation from a registered query descriptor.
    #[must_use]
    pub fn from_descriptor(
        descriptor: &QueryDescriptor,
        actor_id: ActorId,
        parameters: Value,
        result_refs: Vec<QueryResultReference>,
    ) -> Self {
        Self {
            query_id: descriptor.id,
            target: descriptor.target.clone(),
            actor_id,
            parameters,
            result_refs,
            required_capabilities: descriptor.required_capabilities.clone(),
            causation_id: None,
            correlation_id: None,
        }
    }

    /// Adds causation and correlation event metadata.
    #[must_use]
    pub fn with_event_links(
        mut self,
        causation_id: Option<EventId>,
        correlation_id: Option<EventId>,
    ) -> Self {
        self.causation_id = causation_id;
        self.correlation_id = correlation_id;
        self
    }
}

/// Observation message emitted by a subscription.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionMessage {
    /// Subscription that emitted the message.
    pub subscription_id: SubscriptionId,
    /// Monotonic sequence within the subscription stream.
    pub sequence: u64,
    /// Event observed by the subscription, if known.
    pub observed_event_id: Option<EventId>,
    /// Observation payload.
    pub kind: SubscriptionMessageKind,
}

/// Typed subscription update or invalidation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SubscriptionMessageKind {
    /// A dependency changed and may carry bounded read-only update data.
    Updated {
        /// Dependency that changed.
        dependency: SubscriptionDependency,
        /// Optional dependency version after the update.
        version: Option<String>,
        /// Optional bounded read-only payload.
        payload: Option<Value>,
    },
    /// A dependency changed and dependent projections should be refreshed.
    Invalidated {
        /// Dependency that was invalidated.
        dependency: SubscriptionDependency,
        /// Human-readable or adapter-owned invalidation reason.
        reason: Option<String>,
        /// Views known to be dirty because of this invalidation.
        dirty_views: Vec<ViewId>,
    },
    /// Available commands changed for a target.
    CommandAvailabilityChanged {
        /// Target whose command availability changed.
        target: CommandTarget,
        /// Currently available commands for the target.
        available_commands: Vec<CommandId>,
        /// Views known to be dirty because command availability changed.
        dirty_views: Vec<ViewId>,
    },
}

#[cfg(test)]
mod tests {
    use super::{
        CommandInvocation, QueryInvocation, QueryResultReference, SubscriptionMessage,
        SubscriptionMessageKind,
    };
    use crate::{
        ActorId, ArtifactId, CapabilityRequirement, CommandDescriptor, CommandRecoveryPolicy,
        CommandRisk, CommandTarget, DescriptorMetadata, InvocationHintMetadata, ObjectId,
        QueryDescriptor, QueryTarget, SubscriptionDependency, SubscriptionId, ViewId,
    };
    use serde_json::json;

    #[test]
    fn command_invocation_carries_mutation_authority_from_descriptor() {
        let actor_id = ActorId::new();
        let object_id = ObjectId::new();
        let descriptor = CommandDescriptor {
            id: crate::CommandId::new(),
            name: "file.write".to_string(),
            target: CommandTarget::Object { id: object_id },
            args_schema: Some(json!({"type": "object"})),
            required_capabilities: vec![CapabilityRequirement {
                name: "file.write".to_string(),
                reason: Some("write selected file".to_string()),
            }],
            risk: CommandRisk::Medium,
            recovery: CommandRecoveryPolicy::Retryable,
            invocation_hints: InvocationHintMetadata::default(),
            metadata: DescriptorMetadata::new("Write file"),
        };

        let invocation =
            CommandInvocation::from_descriptor(&descriptor, actor_id, json!({"content": "new"}));

        assert_eq!(invocation.command_id, descriptor.id);
        assert_eq!(invocation.actor_id, actor_id);
        assert_eq!(invocation.risk, CommandRisk::Medium);
        assert_eq!(invocation.required_capabilities[0].name, "file.write");
        assert!(matches!(invocation.target, CommandTarget::Object { id } if id == object_id));
    }

    #[test]
    fn query_invocation_only_exposes_read_only_result_references() {
        let artifact_id = ArtifactId::new();
        let descriptor = QueryDescriptor {
            id: crate::QueryId::new(),
            name: "artifact.inspect".to_string(),
            target: QueryTarget::Kernel,
            parameters_schema: None,
            result_schema: Some(json!({"type": "object"})),
            required_capabilities: vec![CapabilityRequirement {
                name: "artifact.read".to_string(),
                reason: None,
            }],
            invocation_hints: InvocationHintMetadata::default(),
            metadata: DescriptorMetadata::new("Inspect artifacts"),
        };

        let invocation = QueryInvocation::from_descriptor(
            &descriptor,
            ActorId::new(),
            json!({}),
            vec![QueryResultReference::Artifact { id: artifact_id }],
        );

        assert_eq!(invocation.query_id, descriptor.id);
        assert_eq!(invocation.required_capabilities[0].name, "artifact.read");
        assert!(matches!(
            invocation.result_refs.as_slice(),
            [QueryResultReference::Artifact { id }] if *id == artifact_id
        ));
    }

    #[test]
    fn subscription_messages_observe_or_invalidate_without_invoking_commands() {
        let view_id = ViewId::new();
        let message = SubscriptionMessage {
            subscription_id: SubscriptionId::new(),
            sequence: 1,
            observed_event_id: None,
            kind: SubscriptionMessageKind::Invalidated {
                dependency: SubscriptionDependency::View { id: view_id },
                reason: Some("task output changed".to_string()),
                dirty_views: vec![view_id],
            },
        };

        assert!(matches!(
            message.kind,
            SubscriptionMessageKind::Invalidated {
                dependency: SubscriptionDependency::View { id },
                dirty_views,
                ..
            } if id == view_id && dirty_views == vec![view_id]
        ));
    }
}
