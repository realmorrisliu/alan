//! Process table (substrate §6.4–§6.5): one `Process` category with identity,
//! parentage, credentials, namespace, status, and exit state — and no
//! `Agent Process` type. Spawn is staged (clone-begin → commit | discard) so a
//! pending slot is never publicly visible until it commits (§7.1a). The table is
//! ephemeral: a fresh table starts empty (D7).

use alan_kernel::{Credentials, ExecSpec, Namespace, ProcessTable, Status};

fn exec(bin: &str) -> ExecSpec {
    ExecSpec {
        executable: bin.to_string(),
        args: vec![],
        namespace: None,
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
}
