//! Namespace engine (substrate §6.1–§6.3, §2.5a): a per-process mount table with
//! mount/bind/unmount, longest-prefix resolution, union directories, read-only
//! vs read-write access, and inheritance where a child may only restrict its own
//! view. Resolution is the *sole* way to reach a resource — there is no global
//! ambient addressing (§6.3): an unmounted path is unreachable.

use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{Fid, InProcessTransport, OpenMode, Request, Response};
use alan_kernel::{Access, Namespace};

fn memfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(MemFs::new()))
}

#[tokio::test]
async fn resolve_routes_a_path_into_the_mounted_tree() {
    let mut ns = Namespace::new();
    ns.mount("/srv/echo", memfs(), Access::ReadWrite);

    let resolved = ns.resolve("/srv/echo/greeting").expect("path is mounted");
    assert_eq!(resolved.rel, vec!["greeting".to_string()]);
    assert_eq!(resolved.access, Access::ReadWrite);

    // The returned tree is live: walking the rel path and reading works.
    resolved
        .tree
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: resolved.rel.clone(),
        })
        .await
        .unwrap();
    resolved
        .tree
        .call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
    let read = resolved
        .tree
        .call(Request::Read {
            fid: Fid(1),
            offset: 0,
            count: 64,
        })
        .await;
    assert_eq!(
        read,
        Ok(Response::Read {
            data: b"hi".to_vec()
        })
    );
}

#[test]
fn union_directory_exposes_every_contributor_at_one_path() {
    // /bin is contributed to by several file servers (e.g. binfs + agent-bin);
    // the namespace presents a union of their mounts at that path.
    let mut ns = Namespace::new();
    ns.mount("/bin", memfs(), Access::ReadOnly);
    ns.mount("/bin", memfs(), Access::ReadOnly);

    let contributors = ns.union_at("/bin");
    assert_eq!(
        contributors.len(),
        2,
        "both contributors remain independent under the union"
    );
}

#[tokio::test]
async fn bind_aliases_a_mounted_tree_under_a_new_path() {
    // The agent-bin tree is posted at /srv/agent-bin and bound into /bin.
    let mut ns = Namespace::new();
    ns.mount("/srv/agent-bin", memfs(), Access::ReadOnly);
    ns.bind("/bin", "/srv/agent-bin");

    let resolved = ns.resolve("/bin/greeting").expect("bound path resolves");
    assert_eq!(resolved.rel, vec!["greeting".to_string()]);
    resolved
        .tree
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: resolved.rel.clone(),
        })
        .await
        .unwrap();
    resolved
        .tree
        .call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
    let read = resolved
        .tree
        .call(Request::Read {
            fid: Fid(1),
            offset: 0,
            count: 64,
        })
        .await;
    assert_eq!(
        read,
        Ok(Response::Read {
            data: b"hi".to_vec()
        })
    );
}

#[tokio::test]
async fn longest_prefix_mount_wins() {
    let mut ns = Namespace::new();
    ns.mount("/", memfs(), Access::ReadOnly);
    ns.mount("/mnt/llm", memfs(), Access::ReadWrite);

    let general = ns.resolve("/greeting").unwrap();
    assert_eq!(general.access, Access::ReadOnly);
    assert_eq!(general.rel, vec!["greeting".to_string()]);

    let specific = ns.resolve("/mnt/llm/greeting").unwrap();
    assert_eq!(specific.access, Access::ReadWrite);
    assert_eq!(specific.rel, vec!["greeting".to_string()]);
}

#[test]
fn unmounted_path_is_unreachable() {
    let ns = Namespace::new();
    assert!(
        ns.resolve("/srv/echo/greeting").is_err(),
        "no global ambient addressing"
    );
}

#[test]
fn read_only_mount_cannot_be_escalated_to_write() {
    let mut ns = Namespace::new();
    ns.mount("/lib", memfs(), Access::ReadOnly);

    let resolved = ns.resolve("/lib/skill").unwrap();
    assert_eq!(resolved.access, Access::ReadOnly);
    assert!(resolved.access.allows(OpenMode::Read));
    assert!(!resolved.access.allows(OpenMode::Write));
    assert!(!resolved.access.allows(OpenMode::ReadWrite));
}

#[test]
fn child_inherits_namespace_and_may_only_restrict_its_own_view() {
    let mut parent = Namespace::new();
    parent.mount("/bin", memfs(), Access::ReadOnly);
    parent.mount("/mnt/llm", memfs(), Access::ReadWrite);

    // A child receives a namespace constructed from the parent's.
    let mut child = parent.child();
    assert!(child.resolve("/mnt/llm/x").is_ok());

    // The child restricts its own view; the parent is unaffected (§ "only that
    // process's view changes").
    child.unmount("/mnt/llm");
    assert!(
        child.resolve("/mnt/llm/x").is_err(),
        "child restricted its own view"
    );
    assert!(
        parent.resolve("/mnt/llm/x").is_ok(),
        "parent view is independent"
    );
}
