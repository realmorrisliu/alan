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

use alan_ap::{ErrorCode, InProcessTransport, OpenMode, Request, Response};

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

/// A successful path resolution: the path components to walk within the backing
/// tree and the access the mount grants. The backing transport is intentionally
/// **private** — callers operate through [`Resolved::call`], which enforces the
/// mount's access, so the boundary cannot be bypassed by reaching the raw tree.
#[derive(Clone)]
pub struct Resolved {
    tree: InProcessTransport,
    pub rel: Vec<String>,
    pub access: Access,
}

impl Resolved {
    /// Carry one operation to the resolved tree, enforcing the mount's access
    /// rights: a read-only mount rejects any mutating request (open-for-write,
    /// write, create, remove) with [`ErrorCode::NoAccess`], so awareness never
    /// implies authority (D6). Read/walk/stat/clunk and read-opens pass through.
    /// This is the only way to reach the backing tree, so the access boundary is
    /// enforced, not advisory.
    pub async fn call(&self, request: Request) -> Result<Response, ErrorCode> {
        if self.access == Access::ReadOnly && is_mutating(&request) {
            return Err(ErrorCode::NoAccess);
        }
        let response = self.tree.call(request).await?;
        // `stat.writable` reports mount-granted authority: a read-only mount masks
        // it to false even if the backing node is writable, so a caller never sees
        // a capability this mount would reject.
        if self.access == Access::ReadOnly
            && let Response::Stat { mut stat } = response
        {
            stat.writable = false;
            return Ok(Response::Stat { stat });
        }
        Ok(response)
    }
}

/// Whether a request would mutate the tree (and so requires write authority).
fn is_mutating(request: &Request) -> bool {
    matches!(
        request,
        Request::Write { .. }
            | Request::Create { .. }
            | Request::Remove { .. }
            | Request::Open {
                mode: OpenMode::Write | OpenMode::ReadWrite,
                ..
            }
    )
}

/// A resolution failure. The kernel keeps a single, namespace-scoped failure
/// reason: the path is not present in this namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreachable;

#[derive(Clone)]
struct Mount {
    /// Absolute path components this tree is mounted at (`/mnt/llm` → `["mnt",
    /// "llm"]`; `/` → `[]`).
    prefix: Vec<String>,
    tree: InProcessTransport,
    access: Access,
}

/// A per-process namespace: an ordered mount table resolved by longest-prefix.
#[derive(Clone)]
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

    pub(crate) fn child_with_path_substitution(&self, token: &str, replacement: &str) -> Namespace {
        let mounts = self
            .mounts
            .iter()
            .map(|m| Mount {
                prefix: m
                    .prefix
                    .iter()
                    .map(|component| component.replace(token, replacement))
                    .collect(),
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

    /// A human/inspectable summary of this namespace's mounts as
    /// `(absolute path, access)` pairs, in mount order. Used to render
    /// `/proc/<pid>/namespace` so a process's capability set is visible there.
    pub fn describe(&self) -> Vec<(String, Access)> {
        self.mounts
            .iter()
            .map(|m| (mount_path(&m.prefix), m.access))
            .collect()
    }

    pub(crate) fn restrict_to_mounts(&self, requested: &[(String, Access)]) -> Option<Namespace> {
        let mut explicit_access = vec![None; self.mounts.len()];
        for (requested_path, requested_access) in requested {
            let requested_prefix = split_path(requested_path);
            let (index, _) = self.mounts.iter().enumerate().find(|(index, mount)| {
                explicit_access[*index].is_none()
                    && mount.prefix == requested_prefix
                    && satisfies_access(mount.access, *requested_access)
            })?;
            explicit_access[index] = Some(*requested_access);
        }

        if has_unsafe_omitted_descendant(&self.mounts, &explicit_access) {
            return None;
        }

        let mut mounts = Vec::new();
        for (index, mount) in self.mounts.iter().enumerate() {
            let Some(requested_access) = explicit_access[index]
                .or_else(|| preserved_overmount_access(&self.mounts, &explicit_access, index))
            else {
                continue;
            };
            let mut restricted = mount.clone();
            restricted.access = requested_access;
            mounts.push(restricted);
        }
        Some(Namespace { mounts })
    }

    /// The union contributors at the **longest** matching prefix for `path`,
    /// most-recently-mounted first. Only equal-prefix contributors are returned —
    /// resolution never falls through from a more-specific overmount to a broader
    /// mount, preserving longest-prefix shadowing (a deeper mount hides what `/`
    /// would otherwise expose). A union directory (several trees at that same
    /// prefix) yields several candidates; the caller walks each in order until one
    /// resolves, so a file present only in an earlier contributor stays reachable.
    pub fn resolve_candidates(&self, path: &str) -> Vec<Resolved> {
        let components = split_path(path);
        let max_len = self
            .mounts
            .iter()
            .filter(|m| is_prefix(&m.prefix, &components))
            .map(|m| m.prefix.len())
            .max();
        let Some(max_len) = max_len else {
            return Vec::new();
        };
        // Only the longest-prefix contributors; most-recently-mounted first.
        self.mounts
            .iter()
            .enumerate()
            .filter(|(_, m)| is_prefix(&m.prefix, &components) && m.prefix.len() == max_len)
            .rev()
            .map(|(_, m)| Resolved {
                tree: m.tree.clone(),
                rel: components[m.prefix.len()..].to_vec(),
                access: m.access,
            })
            .collect()
    }
}

/// Split an absolute path into its non-empty components. `/` → `[]`.
fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn mount_path(prefix: &[String]) -> String {
    if prefix.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", prefix.join("/"))
    }
}

fn satisfies_access(granted: Access, requested: Access) -> bool {
    matches!(
        (granted, requested),
        (Access::ReadWrite, Access::ReadWrite | Access::ReadOnly)
            | (Access::ReadOnly, Access::ReadOnly)
    )
}

fn preserved_overmount_access(
    mounts: &[Mount],
    explicit_access: &[Option<Access>],
    index: usize,
) -> Option<Access> {
    let access_ceiling = nearest_explicit_ancestor_access(mounts, explicit_access, index)?;
    omitted_descendant_is_restrictive(mounts, index, access_ceiling)
        .then_some(restrict_access(mounts[index].access, access_ceiling))
}

fn has_unsafe_omitted_descendant(mounts: &[Mount], explicit_access: &[Option<Access>]) -> bool {
    mounts.iter().enumerate().any(|(index, _)| {
        if explicit_access[index].is_some() {
            return false;
        }
        nearest_explicit_ancestor_access(mounts, explicit_access, index).is_some_and(
            |access_ceiling| !omitted_descendant_is_restrictive(mounts, index, access_ceiling),
        )
    })
}

fn nearest_explicit_ancestor_access(
    mounts: &[Mount],
    explicit_access: &[Option<Access>],
    index: usize,
) -> Option<Access> {
    let descendant = &mounts[index];
    let mut best: Option<(usize, Access)> = None;
    for (ancestor_index, access) in explicit_access.iter().enumerate() {
        let Some(access) = access else {
            continue;
        };
        let ancestor = &mounts[ancestor_index];
        if ancestor.prefix.len() < descendant.prefix.len()
            && is_prefix(&ancestor.prefix, &descendant.prefix)
        {
            match best {
                Some((best_len, _)) if best_len >= ancestor.prefix.len() => {}
                _ => best = Some((ancestor.prefix.len(), *access)),
            }
        }
    }
    best.map(|(_, access)| access)
}

fn omitted_descendant_is_restrictive(
    mounts: &[Mount],
    index: usize,
    access_ceiling: Access,
) -> bool {
    let restricted = restrict_access(mounts[index].access, access_ceiling);
    is_stricter_access(restricted, access_ceiling)
}

fn restrict_access(granted: Access, ceiling: Access) -> Access {
    match (granted, ceiling) {
        (Access::ReadOnly, _) | (_, Access::ReadOnly) => Access::ReadOnly,
        (Access::ReadWrite, Access::ReadWrite) => Access::ReadWrite,
    }
}

fn is_stricter_access(access: Access, ceiling: Access) -> bool {
    matches!((access, ceiling), (Access::ReadOnly, Access::ReadWrite))
}

/// Whether `prefix` is a path-prefix of `components`.
fn is_prefix(prefix: &[String], components: &[String]) -> bool {
    components.len() >= prefix.len() && prefix.iter().zip(components).all(|(a, b)| a == b)
}
