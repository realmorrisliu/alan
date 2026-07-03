//! Content-addressed knowledge store.
//!
//! This crate is the backing model for Ring 3's Venti-inspired knowledge layer:
//! immutable hash-named blocks, Merkle checkpoints, cheap forks, and
//! reachability-based retention. It deliberately does not change the agent-facing
//! file surfaces; `machine/tape`, memory, and context remain file views above
//! this store.

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A SHA-256 content address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash(String);

impl ContentHash {
    /// The stable textual representation, currently `sha256:<hex>`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Access granted by a namespace-bound root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootAccess {
    /// Read and verify the rooted state, but do not mutate the root binding.
    ReadOnly,
    /// Read and update the root binding.
    ReadWrite,
}

/// Retention policy for garbage collection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionPolicy {
    /// Keep unreachable blocks for now.
    KeepUnreachable,
    /// Collect every block and node not reachable from a live or pinned root.
    CollectUnreachable,
}

/// Summary of one GC pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GcReport {
    /// Number of removed raw content blocks.
    pub removed_blocks: usize,
    /// Number of removed Merkle DAG nodes.
    pub removed_nodes: usize,
}

/// A root bound into an agent namespace.
///
/// Possessing a [`ContentHash`] is not enough to read; callers need one of these
/// bindings, issued by the store after the root is made reachable in the
/// namespace model above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundRoot {
    name: String,
    root: ContentHash,
    access: RootAccess,
}

impl BoundRoot {
    /// Namespace-local root name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Checkpoint root hash this binding reaches.
    pub fn root_hash(&self) -> &ContentHash {
        &self.root
    }

    /// Access rights granted by the binding.
    pub fn access(&self) -> RootAccess {
        self.access
    }
}

/// Errors returned by the knowledge store.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KnowledgeError {
    /// The requested raw block is not present.
    #[error("missing block {0}")]
    MissingBlock(ContentHash),
    /// The requested DAG node is not present.
    #[error("missing node {0}")]
    MissingNode(ContentHash),
    /// The caller did not present a live namespace-bound root.
    #[error("root is not reachable through an authorized namespace binding")]
    NoAccess,
    /// Stored bytes no longer match their recorded content hash.
    #[error("content hash mismatch for {0}")]
    HashMismatch(ContentHash),
    /// A root name was not found.
    #[error("unknown root {0}")]
    UnknownRoot(String),
    /// A referenced node cycle was detected.
    #[error("cycle detected at {0}")]
    Cycle(ContentHash),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct DagNodeV1 {
    version: u8,
    #[serde(flatten)]
    kind: DagNodeKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DagNodeKind {
    Sequence { entries: Vec<DagEntry> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "hash", rename_all = "snake_case")]
enum DagEntry {
    Block(ContentHash),
    Node(ContentHash),
}

/// In-memory content-addressed knowledge store.
#[derive(Default)]
pub struct KnowledgeStore {
    blocks: HashMap<ContentHash, Vec<u8>>,
    nodes: HashMap<ContentHash, DagNodeV1>,
    roots: HashMap<String, BoundRoot>,
    pinned_roots: HashSet<ContentHash>,
}

impl KnowledgeStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a raw block under the hash of its bytes.
    ///
    /// Re-writing identical content is idempotent and keeps one physical copy.
    pub fn put_block(&mut self, bytes: impl AsRef<[u8]>) -> ContentHash {
        let bytes = bytes.as_ref();
        let hash = hash_bytes(bytes);
        self.blocks
            .entry(hash.clone())
            .or_insert_with(|| bytes.to_vec());
        hash
    }

    /// Number of stored raw blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Number of stored DAG nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Whether a raw block is present.
    pub fn contains_block(&self, hash: &ContentHash) -> bool {
        self.blocks.contains_key(hash)
    }

    /// Whether a DAG node is present.
    pub fn contains_node(&self, hash: &ContentHash) -> bool {
        self.nodes.contains_key(hash)
    }

    /// Build a checkpoint from existing raw block hashes.
    pub fn checkpoint_from_blocks<I>(&mut self, blocks: I) -> Result<ContentHash, KnowledgeError>
    where
        I: IntoIterator<Item = ContentHash>,
    {
        let entries = blocks
            .into_iter()
            .map(|hash| {
                if self.blocks.contains_key(&hash) {
                    Ok(DagEntry::Block(hash))
                } else {
                    Err(KnowledgeError::MissingBlock(hash))
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.put_node(DagNodeV1 {
            version: 1,
            kind: DagNodeKind::Sequence { entries },
        })
    }

    /// Store blocks and build a checkpoint over them.
    pub fn checkpoint_from_bytes<I, B>(&mut self, blocks: I) -> Result<ContentHash, KnowledgeError>
    where
        I: IntoIterator<Item = B>,
        B: AsRef<[u8]>,
    {
        let hashes = blocks
            .into_iter()
            .map(|block| self.put_block(block))
            .collect::<Vec<_>>();
        self.checkpoint_from_blocks(hashes)
    }

    /// Fork from `base_root` and append existing raw blocks as the divergent
    /// suffix. The fork references the base node, so unchanged blocks are shared.
    pub fn fork_append_blocks<I>(
        &mut self,
        base_root: &ContentHash,
        blocks: I,
    ) -> Result<ContentHash, KnowledgeError>
    where
        I: IntoIterator<Item = ContentHash>,
    {
        if !self.nodes.contains_key(base_root) {
            return Err(KnowledgeError::MissingNode(base_root.clone()));
        }
        let mut entries = vec![DagEntry::Node(base_root.clone())];
        for hash in blocks {
            if !self.blocks.contains_key(&hash) {
                return Err(KnowledgeError::MissingBlock(hash));
            }
            entries.push(DagEntry::Block(hash));
        }
        self.put_node(DagNodeV1 {
            version: 1,
            kind: DagNodeKind::Sequence { entries },
        })
    }

    /// Store new blocks and fork from `base_root` with those blocks appended.
    pub fn fork_append_bytes<I, B>(
        &mut self,
        base_root: &ContentHash,
        blocks: I,
    ) -> Result<ContentHash, KnowledgeError>
    where
        I: IntoIterator<Item = B>,
        B: AsRef<[u8]>,
    {
        let hashes = blocks
            .into_iter()
            .map(|block| self.put_block(block))
            .collect::<Vec<_>>();
        self.fork_append_blocks(base_root, hashes)
    }

    /// Bind a checkpoint root into the namespace model above the store.
    pub fn bind_root(
        &mut self,
        name: impl Into<String>,
        root: ContentHash,
        access: RootAccess,
    ) -> Result<BoundRoot, KnowledgeError> {
        if !self.nodes.contains_key(&root) {
            return Err(KnowledgeError::MissingNode(root));
        }
        let bound = BoundRoot {
            name: name.into(),
            root,
            access,
        };
        self.roots.insert(bound.name.clone(), bound.clone());
        Ok(bound)
    }

    /// Remove a namespace-bound root.
    pub fn unbind_root(&mut self, name: &str) -> Result<(), KnowledgeError> {
        self.roots
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| KnowledgeError::UnknownRoot(name.to_string()))
    }

    /// Resolve a live root by its namespace-local name.
    pub fn root(&self, name: &str) -> Result<BoundRoot, KnowledgeError> {
        self.roots
            .get(name)
            .cloned()
            .ok_or_else(|| KnowledgeError::UnknownRoot(name.to_string()))
    }

    /// Authorize by root hash only if that hash is already reachable through a
    /// live namespace binding.
    pub fn authorize_reachable_hash(
        &self,
        root: &ContentHash,
    ) -> Result<BoundRoot, KnowledgeError> {
        self.roots
            .values()
            .find(|bound| &bound.root == root)
            .cloned()
            .ok_or(KnowledgeError::NoAccess)
    }

    /// Read a materialized file view through an authorized root.
    pub fn read_bound_root(&self, root: &BoundRoot) -> Result<Vec<u8>, KnowledgeError> {
        match self.roots.get(root.name()) {
            Some(current) if current == root => {
                self.verify_root_hash(root.root_hash())?;
                self.materialize_node(root.root_hash(), &mut HashSet::new())
            }
            _ => Err(KnowledgeError::NoAccess),
        }
    }

    /// Pin a root so retention GC keeps its reachable history.
    pub fn pin_root(&mut self, root: &ContentHash) -> Result<(), KnowledgeError> {
        if !self.nodes.contains_key(root) {
            return Err(KnowledgeError::MissingNode(root.clone()));
        }
        self.pinned_roots.insert(root.clone());
        Ok(())
    }

    /// Remove an audit pin.
    pub fn unpin_root(&mut self, root: &ContentHash) {
        self.pinned_roots.remove(root);
    }

    /// Verify the Merkle DAG rooted at `root`.
    pub fn verify_root_hash(&self, root: &ContentHash) -> Result<(), KnowledgeError> {
        self.verify_node(root, &mut HashSet::new())
    }

    /// Collect unreachable content according to `policy`.
    pub fn collect_garbage(&mut self, policy: RetentionPolicy) -> GcReport {
        if policy == RetentionPolicy::KeepUnreachable {
            return GcReport {
                removed_blocks: 0,
                removed_nodes: 0,
            };
        }

        let mut reachable_nodes = HashSet::new();
        let mut reachable_blocks = HashSet::new();
        let roots = self
            .roots
            .values()
            .map(|root| root.root.clone())
            .chain(self.pinned_roots.iter().cloned())
            .collect::<Vec<_>>();
        for root in roots {
            self.mark_reachable(&root, &mut reachable_nodes, &mut reachable_blocks);
        }

        let before_blocks = self.blocks.len();
        let before_nodes = self.nodes.len();
        self.blocks
            .retain(|hash, _| reachable_blocks.contains(hash));
        self.nodes.retain(|hash, _| reachable_nodes.contains(hash));

        GcReport {
            removed_blocks: before_blocks - self.blocks.len(),
            removed_nodes: before_nodes - self.nodes.len(),
        }
    }

    fn put_node(&mut self, node: DagNodeV1) -> Result<ContentHash, KnowledgeError> {
        let bytes = node_bytes(&node);
        let hash = hash_bytes(&bytes);
        self.nodes.entry(hash.clone()).or_insert(node);
        Ok(hash)
    }

    fn materialize_node(
        &self,
        root: &ContentHash,
        visiting: &mut HashSet<ContentHash>,
    ) -> Result<Vec<u8>, KnowledgeError> {
        if !visiting.insert(root.clone()) {
            return Err(KnowledgeError::Cycle(root.clone()));
        }
        let node = self
            .nodes
            .get(root)
            .ok_or_else(|| KnowledgeError::MissingNode(root.clone()))?;
        let mut out = Vec::new();
        match &node.kind {
            DagNodeKind::Sequence { entries } => {
                for entry in entries {
                    match entry {
                        DagEntry::Block(hash) => {
                            let bytes = self
                                .blocks
                                .get(hash)
                                .ok_or_else(|| KnowledgeError::MissingBlock(hash.clone()))?;
                            out.extend_from_slice(bytes);
                        }
                        DagEntry::Node(hash) => {
                            out.extend(self.materialize_node(hash, visiting)?);
                        }
                    }
                }
            }
        }
        visiting.remove(root);
        Ok(out)
    }

    fn verify_node(
        &self,
        root: &ContentHash,
        visiting: &mut HashSet<ContentHash>,
    ) -> Result<(), KnowledgeError> {
        if !visiting.insert(root.clone()) {
            return Err(KnowledgeError::Cycle(root.clone()));
        }
        let node = self
            .nodes
            .get(root)
            .ok_or_else(|| KnowledgeError::MissingNode(root.clone()))?;
        let actual = hash_bytes(&node_bytes(node));
        if &actual != root {
            return Err(KnowledgeError::HashMismatch(root.clone()));
        }
        match &node.kind {
            DagNodeKind::Sequence { entries } => {
                for entry in entries {
                    match entry {
                        DagEntry::Block(hash) => {
                            let bytes = self
                                .blocks
                                .get(hash)
                                .ok_or_else(|| KnowledgeError::MissingBlock(hash.clone()))?;
                            if hash_bytes(bytes) != *hash {
                                return Err(KnowledgeError::HashMismatch(hash.clone()));
                            }
                        }
                        DagEntry::Node(hash) => self.verify_node(hash, visiting)?,
                    }
                }
            }
        }
        visiting.remove(root);
        Ok(())
    }

    fn mark_reachable(
        &self,
        root: &ContentHash,
        nodes: &mut HashSet<ContentHash>,
        blocks: &mut HashSet<ContentHash>,
    ) {
        if !nodes.insert(root.clone()) {
            return;
        }
        let Some(node) = self.nodes.get(root) else {
            return;
        };
        match &node.kind {
            DagNodeKind::Sequence { entries } => {
                for entry in entries {
                    match entry {
                        DagEntry::Block(hash) => {
                            blocks.insert(hash.clone());
                        }
                        DagEntry::Node(hash) => self.mark_reachable(hash, nodes, blocks),
                    }
                }
            }
        }
    }

    /// Corrupt a raw block for verification tests.
    #[doc(hidden)]
    pub fn replace_block_for_test(
        &mut self,
        hash: &ContentHash,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), KnowledgeError> {
        let block = self
            .blocks
            .get_mut(hash)
            .ok_or_else(|| KnowledgeError::MissingBlock(hash.clone()))?;
        *block = bytes.into();
        Ok(())
    }
}

fn node_bytes(node: &DagNodeV1) -> Vec<u8> {
    serde_json::to_vec(node).expect("DAG node serialization is infallible")
}

fn hash_bytes(bytes: &[u8]) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    ContentHash(format!("sha256:{}", hex::encode(hasher.finalize())))
}
