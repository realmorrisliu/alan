//! Process table (substrate §6.4–§6.5): one `Process` category with identity,
//! parentage, credentials, namespace, status, and exit state — and no
//! `Agent Process` type. Spawn is staged (clone-begin → commit | discard) so a
//! pending slot is never publicly visible until it commits (§7.1a). The table is
//! ephemeral: a fresh table starts empty (D7).

use alan_kernel::{
    CANCELLED_EXIT_CODE, Credentials, ExecSpec, ExitReason, Namespace, ProcessExit, ProcessTable,
    Status,
};

fn exec(bin: &str) -> ExecSpec {
    exec_linked(bin, false)
}

fn exec_linked(bin: &str, link_parent: bool) -> ExecSpec {
    ExecSpec {
        executable: bin.to_string(),
        args: vec![],
        namespace: alan_kernel::ExecNamespaceManifest::default(),
        descriptors: Default::default(),
        link_parent,
    }
}

#[test]
fn a_fresh_table_is_empty() {
    let table = ProcessTable::new();
    assert!(
        table.list().is_empty(),
        "the kernel is ephemeral; the table starts empty"
    );
}

#[test]
fn commit_publishes_a_process_with_full_identity() {
    let mut table = ProcessTable::new();
    let parent = table
        .clone_begin(None, Namespace::new(), Credentials::system())
        .and_then(|slot| table.commit(slot, exec("/bin/root")))
        .unwrap();

    let slot = table
        .clone_begin(Some(parent), Namespace::new(), Credentials::user("alan"))
        .unwrap();
    // Pending slot is fid-private: not yet in the public listing.
    assert!(
        !table.list().contains(&slot),
        "a pending slot is not publicly visible"
    );

    let pid = table.commit(slot, exec("/bin/agent")).unwrap();
    assert!(
        table.list().contains(&pid),
        "a committed process becomes public"
    );

    let proc = table.get(pid).unwrap();
    assert_eq!(proc.parent, Some(parent));
    assert_eq!(proc.credentials, Credentials::user("alan"));
    assert_eq!(proc.status, Status::Running);
    assert_eq!(proc.exec.executable, "/bin/agent");
    assert!(proc.exit_code.is_none());
}

#[test]
fn a_discarded_pending_slot_never_becomes_public() {
    let mut table = ProcessTable::new();
    let slot = table
        .clone_begin(None, Namespace::new(), Credentials::system())
        .unwrap();

    table.discard(slot);

    assert!(
        !table.list().contains(&slot),
        "discarded slot leaks nothing into public /proc"
    );
    assert!(table.get(slot).is_none());
}

#[test]
fn exit_records_terminal_status_and_code() {
    let mut table = ProcessTable::new();
    let pid = table
        .clone_begin(None, Namespace::new(), Credentials::system())
        .and_then(|slot| table.commit(slot, exec("/bin/tool")))
        .unwrap();

    table.exit(pid, 0);

    let proc = table.get(pid).unwrap();
    assert_eq!(proc.status, Status::Exited);
    assert_eq!(proc.exit_code, Some(0));

    // A later cancel/termination must not clobber the recorded terminal status.
    table.exit(pid, 130);
    let proc = table.get(pid).unwrap();
    assert_eq!(
        proc.exit_code,
        Some(0),
        "terminal exit code is recorded once"
    );
    assert_eq!(proc.exit_reason, Some(ExitReason::Normal));
}

#[test]
fn exec_spec_deserializes_link_parent_false_by_default() {
    let default_spec: ExecSpec = serde_json::from_str(
        r#"{"executable":"/bin/tool","namespace":{"generation":0,"mounts":[]}}"#,
    )
    .unwrap();
    assert!(!default_spec.link_parent);

    let linked_spec: ExecSpec = serde_json::from_str(
        r#"{"executable":"/bin/tool","namespace":{"generation":0,"mounts":[]},"link_parent":true}"#,
    )
    .unwrap();
    assert!(linked_spec.link_parent);
}

#[test]
fn link_parent_cascades_across_multi_level_descendants_for_any_parent_exit_reason() {
    for (exit_code, reason) in [
        (0, ExitReason::Normal),
        (1, ExitReason::Failed),
        (CANCELLED_EXIT_CODE, ExitReason::Cancelled),
    ] {
        let mut table = ProcessTable::new();
        let root = table
            .clone_begin(None, Namespace::new(), Credentials::system())
            .and_then(|slot| table.commit(slot, exec("/bin/root")))
            .unwrap();
        let linked_child = table
            .clone_begin(Some(root), Namespace::new(), Credentials::user("alan"))
            .and_then(|slot| table.commit(slot, exec_linked("/bin/child", true)))
            .unwrap();
        let unlinked_child = table
            .clone_begin(Some(root), Namespace::new(), Credentials::user("alan"))
            .and_then(|slot| table.commit(slot, exec_linked("/bin/detached", false)))
            .unwrap();
        let linked_grandchild = table
            .clone_begin(
                Some(linked_child),
                Namespace::new(),
                Credentials::user("alan"),
            )
            .and_then(|slot| table.commit(slot, exec_linked("/bin/grandchild", true)))
            .unwrap();
        let linked_great_grandchild = table
            .clone_begin(
                Some(linked_grandchild),
                Namespace::new(),
                Credentials::user("alan"),
            )
            .and_then(|slot| table.commit(slot, exec_linked("/bin/great-grandchild", true)))
            .unwrap();
        let child_under_unlinked = table
            .clone_begin(
                Some(unlinked_child),
                Namespace::new(),
                Credentials::user("alan"),
            )
            .and_then(|slot| table.commit(slot, exec_linked("/bin/under-detached", true)))
            .unwrap();
        let already_exited_linked = table
            .clone_begin(Some(root), Namespace::new(), Credentials::user("alan"))
            .and_then(|slot| table.commit(slot, exec_linked("/bin/done", true)))
            .unwrap();
        table.exit(already_exited_linked, 0);

        let exits = table.exit_with_reason(root, exit_code, reason);
        assert_eq!(
            exits,
            vec![
                ProcessExit {
                    pid: root,
                    exit_code,
                    reason,
                },
                ProcessExit {
                    pid: linked_child,
                    exit_code: CANCELLED_EXIT_CODE,
                    reason: ExitReason::Cancelled,
                },
                ProcessExit {
                    pid: linked_grandchild,
                    exit_code: CANCELLED_EXIT_CODE,
                    reason: ExitReason::Cancelled,
                },
                ProcessExit {
                    pid: linked_great_grandchild,
                    exit_code: CANCELLED_EXIT_CODE,
                    reason: ExitReason::Cancelled,
                },
            ],
            "cascade must cancel linked tree deterministically for parent exit {reason:?}"
        );

        // Unlinked branch remains untouched.
        assert_eq!(table.get(unlinked_child).unwrap().status, Status::Running);
        assert_eq!(
            table.get(child_under_unlinked).unwrap().status,
            Status::Running
        );

        // Already-exited linked sibling retains original terminal state.
        let done = table.get(already_exited_linked).unwrap();
        assert_eq!(done.exit_code, Some(0));
        assert_eq!(done.exit_reason, Some(ExitReason::Normal));

        // Later exiting the unlinked child cancels its own linked child.
        let later_exits = table.exit_with_reason(unlinked_child, 0, ExitReason::Normal);
        assert_eq!(
            later_exits,
            vec![
                ProcessExit {
                    pid: unlinked_child,
                    exit_code: 0,
                    reason: ExitReason::Normal,
                },
                ProcessExit {
                    pid: child_under_unlinked,
                    exit_code: CANCELLED_EXIT_CODE,
                    reason: ExitReason::Cancelled,
                },
            ]
        );
    }
}

#[test]
fn committing_linked_child_after_parent_exit_marks_child_cancelled_immediately() {
    let mut table = ProcessTable::new();
    let parent = table
        .clone_begin(None, Namespace::new(), Credentials::system())
        .and_then(|slot| table.commit(slot, exec("/bin/parent")))
        .unwrap();
    let pending_child = table
        .clone_begin(Some(parent), Namespace::new(), Credentials::user("alan"))
        .unwrap();

    table.exit(parent, 0);

    let child = table
        .commit(pending_child, exec_linked("/bin/child", true))
        .unwrap();
    let proc = table.get(child).unwrap();
    assert_eq!(proc.status, Status::Exited);
    assert_eq!(proc.exit_code, Some(CANCELLED_EXIT_CODE));
    assert_eq!(proc.exit_reason, Some(ExitReason::Cancelled));
}
