use crate::{
    CommandDescriptor, CommandId, CommandTarget, QueryDescriptor, QueryId, QueryTarget,
    SubscriptionDependency, SubscriptionDescriptor, SubscriptionId,
};
use std::collections::BTreeMap;

/// Registry for command descriptors shared by UI, agents, automation, and hosts.
pub trait CommandRegistry {
    /// Registers or replaces a command descriptor.
    fn register_command(&mut self, descriptor: CommandDescriptor) -> Option<CommandDescriptor>;

    /// Looks up a command descriptor by id.
    fn command(&self, id: CommandId) -> Option<&CommandDescriptor>;

    /// Lists all registered command descriptors.
    fn commands(&self) -> Vec<&CommandDescriptor>;

    /// Lists commands whose descriptor target exactly matches the requested target.
    fn commands_for_target(&self, target: &CommandTarget) -> Vec<&CommandDescriptor>;
}

/// Registry for read-only query descriptors.
pub trait QueryRegistry {
    /// Registers or replaces a query descriptor.
    fn register_query(&mut self, descriptor: QueryDescriptor) -> Option<QueryDescriptor>;

    /// Looks up a query descriptor by id.
    fn query(&self, id: QueryId) -> Option<&QueryDescriptor>;

    /// Lists all registered query descriptors.
    fn queries(&self) -> Vec<&QueryDescriptor>;

    /// Lists queries whose descriptor target exactly matches the requested target.
    fn queries_for_target(&self, target: &QueryTarget) -> Vec<&QueryDescriptor>;
}

/// Registry for subscription descriptors.
pub trait SubscriptionRegistry {
    /// Registers or replaces a subscription descriptor.
    fn register_subscription(
        &mut self,
        descriptor: SubscriptionDescriptor,
    ) -> Option<SubscriptionDescriptor>;

    /// Looks up a subscription descriptor by id.
    fn subscription(&self, id: SubscriptionId) -> Option<&SubscriptionDescriptor>;

    /// Lists all registered subscription descriptors.
    fn subscriptions(&self) -> Vec<&SubscriptionDescriptor>;

    /// Lists subscriptions that observe the requested dependency.
    fn subscriptions_for_dependency(
        &self,
        dependency: &SubscriptionDependency,
    ) -> Vec<&SubscriptionDescriptor>;
}

/// Lightweight in-memory command registry for tests and early adapters.
#[derive(Clone, Debug, Default)]
pub struct InMemoryCommandRegistry {
    commands: BTreeMap<CommandId, CommandDescriptor>,
}

impl InMemoryCommandRegistry {
    /// Creates an empty command registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl CommandRegistry for InMemoryCommandRegistry {
    fn register_command(&mut self, descriptor: CommandDescriptor) -> Option<CommandDescriptor> {
        self.commands.insert(descriptor.id, descriptor)
    }

    fn command(&self, id: CommandId) -> Option<&CommandDescriptor> {
        self.commands.get(&id)
    }

    fn commands(&self) -> Vec<&CommandDescriptor> {
        self.commands.values().collect()
    }

    fn commands_for_target(&self, target: &CommandTarget) -> Vec<&CommandDescriptor> {
        self.commands
            .values()
            .filter(|descriptor| &descriptor.target == target)
            .collect()
    }
}

/// Lightweight in-memory query registry for tests and early adapters.
#[derive(Clone, Debug, Default)]
pub struct InMemoryQueryRegistry {
    queries: BTreeMap<QueryId, QueryDescriptor>,
}

impl InMemoryQueryRegistry {
    /// Creates an empty query registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl QueryRegistry for InMemoryQueryRegistry {
    fn register_query(&mut self, descriptor: QueryDescriptor) -> Option<QueryDescriptor> {
        self.queries.insert(descriptor.id, descriptor)
    }

    fn query(&self, id: QueryId) -> Option<&QueryDescriptor> {
        self.queries.get(&id)
    }

    fn queries(&self) -> Vec<&QueryDescriptor> {
        self.queries.values().collect()
    }

    fn queries_for_target(&self, target: &QueryTarget) -> Vec<&QueryDescriptor> {
        self.queries
            .values()
            .filter(|descriptor| &descriptor.target == target)
            .collect()
    }
}

/// Lightweight in-memory subscription registry for tests and early adapters.
#[derive(Clone, Debug, Default)]
pub struct InMemorySubscriptionRegistry {
    subscriptions: BTreeMap<SubscriptionId, SubscriptionDescriptor>,
}

impl InMemorySubscriptionRegistry {
    /// Creates an empty subscription registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl SubscriptionRegistry for InMemorySubscriptionRegistry {
    fn register_subscription(
        &mut self,
        descriptor: SubscriptionDescriptor,
    ) -> Option<SubscriptionDescriptor> {
        self.subscriptions.insert(descriptor.id, descriptor)
    }

    fn subscription(&self, id: SubscriptionId) -> Option<&SubscriptionDescriptor> {
        self.subscriptions.get(&id)
    }

    fn subscriptions(&self) -> Vec<&SubscriptionDescriptor> {
        self.subscriptions.values().collect()
    }

    fn subscriptions_for_dependency(
        &self,
        dependency: &SubscriptionDependency,
    ) -> Vec<&SubscriptionDescriptor> {
        self.subscriptions
            .values()
            .filter(|descriptor| {
                descriptor
                    .dependencies
                    .iter()
                    .any(|item| item == dependency)
            })
            .collect()
    }
}

/// Combined in-memory Kernel surface registry.
#[derive(Clone, Debug, Default)]
pub struct InMemoryKernelRegistry {
    /// Command descriptor registry.
    pub commands: InMemoryCommandRegistry,
    /// Query descriptor registry.
    pub queries: InMemoryQueryRegistry,
    /// Subscription descriptor registry.
    pub subscriptions: InMemorySubscriptionRegistry,
}

impl InMemoryKernelRegistry {
    /// Creates an empty combined registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandRegistry, InMemoryCommandRegistry, InMemoryQueryRegistry,
        InMemorySubscriptionRegistry, QueryRegistry, SubscriptionRegistry,
    };
    use crate::{
        CommandDescriptor, CommandRisk, CommandTarget, DescriptorMetadata, InvocationHintMetadata,
        ObjectId, QueryDescriptor, QueryTarget, SubscriptionDependency, SubscriptionDescriptor,
        ViewId,
    };

    #[test]
    fn registries_index_commands_queries_and_subscriptions_by_semantic_target() {
        let object_id = ObjectId::new();
        let mut command_registry = InMemoryCommandRegistry::new();
        let command = CommandDescriptor {
            id: crate::CommandId::new(),
            name: "object.open".to_string(),
            target: CommandTarget::Object { id: object_id },
            args_schema: None,
            required_capabilities: Vec::new(),
            risk: CommandRisk::Low,
            recovery: crate::CommandRecoveryPolicy::None,
            invocation_hints: InvocationHintMetadata::default(),
            metadata: DescriptorMetadata::new("Open object"),
        };
        command_registry.register_command(command.clone());

        assert_eq!(
            command_registry.commands_for_target(&CommandTarget::Object { id: object_id }),
            vec![&command]
        );

        let mut query_registry = InMemoryQueryRegistry::new();
        let query = QueryDescriptor {
            id: crate::QueryId::new(),
            name: "object.read".to_string(),
            target: QueryTarget::Object { id: object_id },
            parameters_schema: None,
            result_schema: None,
            required_capabilities: Vec::new(),
            invocation_hints: InvocationHintMetadata::default(),
            metadata: DescriptorMetadata::new("Read object"),
        };
        query_registry.register_query(query.clone());

        assert_eq!(
            query_registry.queries_for_target(&QueryTarget::Object { id: object_id }),
            vec![&query]
        );

        let view_id = ViewId::new();
        let dependency = SubscriptionDependency::View { id: view_id };
        let mut subscription_registry = InMemorySubscriptionRegistry::new();
        let subscription = SubscriptionDescriptor {
            id: crate::SubscriptionId::new(),
            name: "view.dirty".to_string(),
            dependencies: vec![dependency.clone()],
            required_capabilities: Vec::new(),
            metadata: DescriptorMetadata::new("View dirty"),
        };
        subscription_registry.register_subscription(subscription.clone());

        assert_eq!(
            subscription_registry.subscriptions_for_dependency(&dependency),
            vec![&subscription]
        );
    }
}
