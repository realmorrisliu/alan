//! The per-process namespace engine: a mount table assembled by mount/bind/
//! union over file servers, resolved by longest-prefix walk.
//!
//! The namespace is the **sole capability boundary** (ADR-0024 D6): a resource
//! is reachable iff it is present in this namespace. There is no global ambient
//! addressing — an unmounted path simply does not resolve. Access rights
//! separate awareness ([`Access::ReadOnly`]: walk/read/watch) from authority
//! ([`Access::ReadWrite`]: mutation); a read-only mount cannot be escalated to
//! write from within the namespace (§2.5a).
//!
//! A child namespace is constructed from its spawner's ([`Namespace::child`])
//! and may only *restrict* its own view; changes never affect another
//! namespace. Mount state is ephemeral kernel runtime state (D7).

use alan_ap::{InProcessTransport, OpenMode};

/// Whether a mount grants only awareness or also authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Awareness: walk, read, and watch — but no mutation.
    ReadOnly,
    /// Authority: read and mutate.
    ReadWrite,
}

impl Access {
    /// Whether this access permits opening with `mode`. A read-only mount denies
    /// any write intent, so it can never be escalated to write.
    pub fn allows(self, mode: OpenMode) -> bool {
        match (self, mode) {
            (Access::ReadWrite, _) => true,
            (Access::ReadOnly, OpenMode::Read) => true,
            (Access::ReadOnly, OpenMode::Write | OpenMode::ReadWrite) => false,
        }
    }
}

/// A successful path resolution: the file server backing the path, the path
/// components to walk within it, and the access the mount grants.
#[derive(Clone)]
pub struct Resolved {
    pub tree: InProcessTransport,
    pub rel: Vec<String>,
    pub access: Access,
}

/// A resolution failure. The kernel keeps a single, namespace-scoped failure
/// reason: the path is not present in this namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreachable;

struct Mount {
    /// Absolute path components this tree is mounted at (`/mnt/llm` → `["mnt",
    /// "llm"]`; `/` → `[]`).
    prefix: Vec<String>,
    tree: InProcessTransport,
    access: Access,
}

/// A per-process namespace: an ordered mount table resolved by longest-prefix.
pub struct Namespace {
    mounts: Vec<Mount>,
}

impl Default for Namespace {
    fn default() -> Self {
        Self::new()
    }
}

impl Namespace {
    pub fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    /// Mount `tree` at absolute path `at` with the given access. A later mount at
    /// the same prefix shadows an earlier one (last wins), which is how a
    /// namespace is re-assembled.
    pub fn mount(&mut self, at: &str, tree: InProcessTransport, access: Access) {
        self.mounts.push(Mount {
            prefix: split_path(at),
            tree,
            access,
        });
    }

    /// Bind the tree(s) mounted at `existing` under the new path `new` (an
    /// alias). This is how a union root such as `/bin` is assembled from posted
    /// handles like `/srv/bin` and `/srv/agent-bin`. v1 aliases whole mount
    /// points (the bound prefix must name a mount), preserving each tree's
    /// access.
    pub fn bind(&mut self, new: &str, existing: &str) {
        let existing_prefix = split_path(existing);
        let new_prefix = split_path(new);
        let aliases: Vec<Mount> = self
            .mounts
            .iter()
            .filter(|m| m.prefix == existing_prefix)
            .map(|m| Mount {
                prefix: new_prefix.clone(),
                tree: m.tree.clone(),
                access: m.access,
            })
            .collect();
        self.mounts.extend(aliases);
    }

    /// Remove every mount at exactly `at`. Restricting one's own view never
    /// affects another namespace.
    pub fn unmount(&mut self, at: &str) {
        let prefix = split_path(at);
        self.mounts.retain(|m| m.prefix != prefix);
    }

    /// Every tree mounted at exactly `path`, in mount order. A union directory
    /// has more than one; a lister merges their entries while the contributors
    /// stay independent (§ "A standard root is assembled").
    pub fn union_at(&self, path: &str) -> Vec<Resolved> {
        let prefix = split_path(path);
        self.mounts
            .iter()
            .filter(|m| m.prefix == prefix)
            .map(|m| Resolved {
                tree: m.tree.clone(),
                rel: Vec::new(),
                access: m.access,
            })
            .collect()
    }

    /// Construct a child namespace from this one. The child inherits the current
    /// mounts and may only restrict its own view thereafter.
    pub fn child(&self) -> Namespace {
        let mounts = self
            .mounts
            .iter()
            .map(|m| Mount {
                prefix: m.prefix.clone(),
                tree: m.tree.clone(),
                access: m.access,
            })
            .collect();
        Namespace { mounts }
    }

    /// Resolve an absolute path to its backing tree and the components to walk
    /// within it. The mount with the longest matching prefix wins; among equal
    /// prefixes, the most recent mount wins. An unmounted path is [`Unreachable`].
    pub fn resolve(&self, path: &str) -> Result<Resolved, Unreachable> {
        let components = split_path(path);
        let mut best: Option<&Mount> = None;
        for mount in &self.mounts {
            if is_prefix(&mount.prefix, &components) {
                let better = match best {
                    Some(b) => mount.prefix.len() >= b.prefix.len(),
                    None => true,
                };
                if better {
                    best = Some(mount);
                }
            }
        }
        let mount = best.ok_or(Unreachable)?;
        Ok(Resolved {
            tree: mount.tree.clone(),
            rel: components[mount.prefix.len()..].to_vec(),
            access: mount.access,
        })
    }
}

/// Split an absolute path into its non-empty components. `/` → `[]`.
fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Whether `prefix` is a path-prefix of `components`.
fn is_prefix(prefix: &[String], components: &[String]) -> bool {
    components.len() >= prefix.len() && prefix.iter().zip(components).all(|(a, b)| a == b)
}
