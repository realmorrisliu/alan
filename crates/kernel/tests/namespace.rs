//! Namespace engine (substrate §6.1–§6.3, §2.5a): a per-process mount table with
//! mount/bind/unmount, longest-prefix resolution, union directories, read-only
//! vs read-write access, and inheritance where a child may only restrict its own
//! view. Resolution is the *sole* way to reach a resource — there is no global
//! ambient addressing (§6.3): an unmounted path is unreachable.

use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Request,
    Response, Stat,
};
use alan_kernel::{Access, Namespace};

fn memfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(MemFs::new()))
}

/// A server with no files — every walk fails. Used to model a union contributor
/// that does not hold the requested file.
struct EmptyFs;

#[async_trait::async_trait]
impl FileServer for EmptyFs {
    async fn walk(&self, _: Fid, _: Fid, _: &[String]) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn open(&self, _: Fid, _: OpenMode) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn read(&self, _: Fid, _: Offset, _: u32) -> Result<Vec<u8>, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn write(&self, _: Fid, _: Offset, _: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn stat(&self, _: Fid) -> Result<Stat, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn create(&self, _: Fid, _: Fid, _: &str, _: FileKind) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn remove(&self, _: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::NotFound)
    }
    async fn clunk(&self, _: Fid) -> Result<(), ErrorCode> {
        Ok(())
    }
}

fn emptyfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(EmptyFs))
}

// §2.5a / D6 — a read-only mount enforces awareness-only: mutating calls through
// the resolved handle are denied, not merely advisory (PR #574 review).
#[tokio::test]
async fn read_only_mount_enforces_access_on_calls() {
    let mut ns = Namespace::new();
    ns.mount("/lib", memfs(), Access::ReadOnly);
    ns.mount("/mnt/llm", memfs(), Access::ReadWrite);

    let ro = ns.resolve("/lib/submit").unwrap();
    // Read-opens and reads pass through.
    ro.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(1),
        names: vec!["greeting".into()],
    })
    .await
    .unwrap();
    assert!(matches!(
        ro.call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read
        })
        .await,
        Ok(Response::Open { .. })
    ));
    // Mutating calls are rejected by the mount, before reaching the tree.
    assert_eq!(
        ro.call(Request::Open {
            fid: Fid(2),
            mode: OpenMode::Write
        })
        .await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        ro.call(Request::Write {
            fid: Fid(1),
            offset: 0,
            data: b"x".to_vec()
        })
        .await,
        Err(ErrorCode::NoAccess)
    );

    // A read-write mount allows the same mutating call to reach the tree.
    let rw = ns.resolve("/mnt/llm/submit").unwrap();
    rw.call(Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(3),
        names: vec!["submit".into()],
    })
    .await
    .unwrap();
    assert!(matches!(
        rw.call(Request::Open {
            fid: Fid(3),
            mode: OpenMode::Write
        })
        .await,
        Ok(Response::Open { .. })
    ));
}

// A union directory's earlier contributor stays reachable: resolve_candidates
// returns every contributor so the caller can search past a last-mounted tree
// that lacks the file (PR #574 review).
#[tokio::test]
async fn resolve_candidates_searches_union_contributors() {
    let mut ns = Namespace::new();
    ns.mount("/bin", memfs(), Access::ReadOnly); // earlier: has "greeting"
    ns.mount("/bin", emptyfs(), Access::ReadOnly); // later: has nothing

    let candidates = ns.resolve_candidates("/bin/greeting");
    assert_eq!(candidates.len(), 2, "both union contributors are returned");
    for c in &candidates {
        assert_eq!(c.rel, vec!["greeting".to_string()]);
    }

    // Walking the most-recent (empty) candidate fails; the earlier one resolves —
    // so the file is reachable instead of shadowed by last-wins.
    assert!(
        candidates[0]
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["greeting".into()]
            })
            .await
            .is_err(),
        "the last-mounted contributor lacks the file"
    );
    candidates[1]
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(2),
            names: vec!["greeting".into()],
        })
        .await
        .expect("an earlier contributor still serves the file");
}

// resolve_candidates keeps only the longest-prefix contributors — a deeper
// overmount shadows the broader mount, never falls through to it (PR #574 review).
#[tokio::test]
async fn resolve_candidates_keeps_only_the_longest_prefix() {
    let mut ns = Namespace::new();
    ns.mount("/", memfs(), Access::ReadOnly); // broad: has "greeting"
    ns.mount("/mnt/llm", emptyfs(), Access::ReadOnly); // deeper overmount: empty

    let candidates = ns.resolve_candidates("/mnt/llm/greeting");
    assert_eq!(
        candidates.len(),
        1,
        "only the longest-prefix mount, no fall-through to /"
    );
    assert!(
        candidates[0]
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: Fid(1),
                names: vec!["greeting".into()]
            })
            .await
            .is_err(),
        "the deeper overmount shadows the broader mount"
    );
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
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: resolved.rel.clone(),
        })
        .await
        .unwrap();
    resolved
        .call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
    let read = resolved
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
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: Fid(1),
            names: resolved.rel.clone(),
        })
        .await
        .unwrap();
    resolved
        .call(Request::Open {
            fid: Fid(1),
            mode: OpenMode::Read,
        })
        .await
        .unwrap();
    let read = resolved
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
