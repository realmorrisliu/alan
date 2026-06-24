use serde::{Deserialize, Serialize};

/// Points at a source-of-truth resource outside Alan Kernel identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NativeReference {
    /// A local filesystem path remains authoritative for file contents.
    File(FileReference),
    /// A Git worktree or repository remains authoritative for repository state.
    GitRepository(GitRepositoryReference),
    /// An adapter-owned agent session remains authoritative for agent runtime state.
    AgentSession(AgentSessionReference),
    /// A terminal host remains authoritative for terminal process and pty state.
    TerminalHandle(TerminalHandleReference),
    /// An app or domain store remains authoritative for domain-owned state.
    DomainResource(DomainResourceReference),
}

/// Native authority for a local file or directory.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileReference {
    /// Host-local path to the file or directory.
    pub path: String,
    /// Optional content revision, digest, or stat-derived version known to the adapter.
    pub version: Option<String>,
}

/// Native authority for a Git repository or worktree.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GitRepositoryReference {
    /// Host-local path to the worktree.
    pub worktree_path: String,
    /// Optional remote URL or repository identity.
    pub repository: Option<String>,
    /// Optional revision, branch, or content digest.
    pub revision: Option<String>,
}

/// Native authority for an adapter-owned agent session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentSessionReference {
    /// Adapter family that owns the session reference.
    pub adapter: String,
    /// Adapter-owned session id.
    pub session_id: String,
}

/// Native authority for a terminal host handle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TerminalHandleReference {
    /// Host or adapter that owns the terminal handle.
    pub host: String,
    /// Host-owned terminal handle id.
    pub handle_id: String,
}

/// Native authority for app-owned or domain-owned resources.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DomainResourceReference {
    /// Domain or Alan App that owns the resource.
    pub domain: String,
    /// Domain-owned resource kind.
    pub resource_kind: String,
    /// Domain-owned resource id.
    pub resource_id: String,
    /// Optional domain-owned version, revision, or digest.
    pub version: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{AgentSessionReference, NativeReference};

    #[test]
    fn agent_session_native_ref_preserves_adapter_authority() {
        let native_ref = NativeReference::AgentSession(AgentSessionReference {
            adapter: "alan-agent".to_string(),
            session_id: "session-123".to_string(),
        });

        let NativeReference::AgentSession(session) = native_ref else {
            panic!("expected agent session native reference");
        };

        assert_eq!(session.adapter, "alan-agent");
        assert_eq!(session.session_id, "session-123");
    }
}
