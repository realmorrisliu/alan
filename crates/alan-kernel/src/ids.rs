use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a fresh opaque id.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID as this id type.
            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// Returns the wrapped UUID value.
            #[must_use]
            pub fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

typed_id!(
    #[doc = "Identifies a human, agent, extension, or system actor."]
    ActorId
);
typed_id!(
    #[doc = "Identifies an inspectable Alan Kernel object."]
    ObjectId
);
typed_id!(
    #[doc = "Identifies an active work context over objects, tasks, or query results."]
    BufferId
);
typed_id!(
    #[doc = "Identifies a semantic presentation of a buffer."]
    ViewId
);
typed_id!(
    #[doc = "Identifies a command descriptor shared across invocation surfaces."]
    CommandId
);
typed_id!(
    #[doc = "Identifies a read-only semantic query."]
    QueryId
);
typed_id!(
    #[doc = "Identifies an observation or invalidation subscription."]
    SubscriptionId
);
typed_id!(
    #[doc = "Identifies command execution or long-running work."]
    TaskId
);
typed_id!(
    #[doc = "Identifies a produced Alan Kernel artifact."]
    ArtifactId
);
typed_id!(
    #[doc = "Identifies evidence attached to a task, artifact, or decision."]
    EvidenceId
);
typed_id!(
    #[doc = "Identifies an activity ledger event."]
    EventId
);
typed_id!(
    #[doc = "Identifies a bounded Agent Capability run."]
    AgentRunId
);
typed_id!(
    #[doc = "Identifies a Context Grant supplied to an Agent Run."]
    ContextGrantId
);
typed_id!(
    #[doc = "Identifies a Result Contract requested from an Agent Run."]
    ResultContractId
);
typed_id!(
    #[doc = "Identifies Execution Guard metadata recorded for governance."]
    ExecutionGuardId
);
typed_id!(
    #[doc = "Identifies a command or Agent Run audit record."]
    AuditRecordId
);

#[cfg(test)]
mod tests {
    use super::{ActorId, ObjectId};
    use uuid::Uuid;

    #[test]
    fn typed_ids_preserve_uuid_values_without_sharing_types() {
        let raw = Uuid::nil();

        fn accepts_actor(_: ActorId) {}
        fn accepts_object(_: ObjectId) {}

        let actor_id = ActorId::from_uuid(raw);
        let object_id = ObjectId::from_uuid(raw);

        accepts_actor(actor_id);
        accepts_object(object_id);

        assert_eq!(actor_id.as_uuid(), raw);
        assert_eq!(object_id.as_uuid(), raw);
        assert_eq!(actor_id.to_string(), object_id.to_string());
    }
}
