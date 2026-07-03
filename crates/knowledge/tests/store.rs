use alan_knowledge::{KnowledgeError, KnowledgeStore, RetentionPolicy, RootAccess};

#[test]
fn identical_blocks_are_deduplicated_by_content_hash() {
    let mut store = KnowledgeStore::new();

    let first = store.put_block(b"same context");
    let second = store.put_block(b"same context");

    assert_eq!(first, second);
    assert_eq!(store.block_count(), 1);
    assert!(first.as_str().starts_with("sha256:"));
}

#[test]
fn checkpoint_materializes_and_verifies_root_hash() {
    let mut store = KnowledgeStore::new();
    let root = store
        .checkpoint_from_bytes([b"record-1\n".as_slice(), b"record-2\n".as_slice()])
        .unwrap();
    let bound = store
        .bind_root("agent/root/machine", root.clone(), RootAccess::ReadOnly)
        .unwrap();

    assert_eq!(
        store.read_bound_root(&bound).unwrap(),
        b"record-1\nrecord-2\n"
    );
    store.verify_root_hash(&root).unwrap();
}

#[test]
fn fork_shares_unchanged_blocks_and_writes_only_delta() {
    let mut store = KnowledgeStore::new();
    let base = store
        .checkpoint_from_bytes([b"a\n".as_slice(), b"b\n".as_slice()])
        .unwrap();
    let initial_blocks = store.block_count();
    let initial_nodes = store.node_count();

    let fork = store.fork_append_bytes(&base, [b"c\n".as_slice()]).unwrap();
    let base_bound = store.bind_root("base", base, RootAccess::ReadOnly).unwrap();
    let fork_bound = store.bind_root("fork", fork, RootAccess::ReadOnly).unwrap();

    assert_eq!(store.block_count(), initial_blocks + 1);
    assert_eq!(store.node_count(), initial_nodes + 1);
    assert_eq!(store.read_bound_root(&base_bound).unwrap(), b"a\nb\n");
    assert_eq!(store.read_bound_root(&fork_bound).unwrap(), b"a\nb\nc\n");
}

#[test]
fn content_hash_does_not_authorize_reads_without_reachable_root() {
    let mut store = KnowledgeStore::new();
    let root = store
        .checkpoint_from_bytes([b"private\n".as_slice()])
        .unwrap();

    assert_eq!(
        store.authorize_reachable_hash(&root),
        Err(KnowledgeError::NoAccess)
    );

    let bound = store
        .bind_root("agent/private", root.clone(), RootAccess::ReadOnly)
        .unwrap();
    assert_eq!(store.authorize_reachable_hash(&root).unwrap(), bound);
    assert_eq!(store.read_bound_root(&bound).unwrap(), b"private\n");

    store.unbind_root("agent/private").unwrap();
    assert_eq!(store.read_bound_root(&bound), Err(KnowledgeError::NoAccess));
}

#[test]
fn tampering_with_a_block_is_detected_by_verification() {
    let mut store = KnowledgeStore::new();
    let block = store.put_block(b"original");
    let root = store.checkpoint_from_blocks([block.clone()]).unwrap();

    store.replace_block_for_test(&block, b"rewritten").unwrap();

    assert_eq!(
        store.verify_root_hash(&root),
        Err(KnowledgeError::HashMismatch(block))
    );
}

#[test]
fn gc_collects_unreachable_blocks_but_keeps_live_and_pinned_roots() {
    let mut store = KnowledgeStore::new();
    let live = store.checkpoint_from_bytes([b"live".as_slice()]).unwrap();
    let pinned = store.checkpoint_from_bytes([b"pinned".as_slice()]).unwrap();
    let garbage = store
        .checkpoint_from_bytes([b"garbage".as_slice()])
        .unwrap();

    store
        .bind_root("live", live.clone(), RootAccess::ReadOnly)
        .unwrap();
    store.pin_root(&pinned).unwrap();

    let report = store.collect_garbage(RetentionPolicy::CollectUnreachable);

    assert_eq!(report.removed_blocks, 1);
    assert_eq!(report.removed_nodes, 1);
    assert!(store.contains_node(&live));
    assert!(store.contains_node(&pinned));
    assert!(!store.contains_node(&garbage));

    store.unbind_root("live").unwrap();
    store.unpin_root(&pinned);
    let report = store.collect_garbage(RetentionPolicy::CollectUnreachable);
    assert_eq!(report.removed_nodes, 2);
    assert_eq!(report.removed_blocks, 2);
}
