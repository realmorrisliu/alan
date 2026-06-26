//! `/srv` — the bootstrap rendezvous device (substrate §7.2). File servers post
//! mountable handles here; a process sees and mounts only the handles its access
//! permits. `/srv` is not an ambient backdoor: a service withheld from a process
//! (its handle filtered out) is **not** remountable via `/srv`, which is what
//! makes denial-by-absent-mount hold (D6).

use std::collections::HashSet;
use std::sync::Arc;

use alan_ap::reference::MemFs;
use alan_ap::{Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, SrvFs};

fn memfs() -> InProcessTransport {
    InProcessTransport::new(Arc::new(MemFs::new()))
}

#[tokio::test]
async fn a_posted_handle_is_discoverable_and_mountable() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadWrite).await;
    srv.post("mem", memfs(), Access::ReadOnly).await;

    assert_eq!(srv.list().await, vec!["llm".to_string(), "mem".to_string()]);

    // Looking a handle up yields the mountable tree and its access.
    let (tree, access) = srv.lookup("llm").await.expect("posted handle is mountable");
    assert_eq!(access, Access::ReadWrite);
    tree.call(alan_ap::Request::Walk {
        fid: Fid::ROOT,
        newfid: Fid(1),
        names: vec!["greeting".into()],
    })
    .await
    .expect("the handle resolves to a live tree");
}

#[tokio::test]
async fn a_withheld_handle_is_filtered_and_not_remountable() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadWrite).await;
    srv.post("mem", memfs(), Access::ReadOnly).await;

    // A restricted child's /srv view withholds "llm".
    let denied: HashSet<String> = ["llm".to_string()].into_iter().collect();
    let view = srv.view(&denied).await;

    assert_eq!(
        view.list(),
        vec!["mem".to_string()],
        "withheld handle is absent from the view"
    );
    assert!(
        view.lookup("mem").is_some(),
        "permitted handle still mounts"
    );
    assert!(
        view.lookup("llm").is_none(),
        "withheld service cannot be regained via /srv"
    );
}

#[tokio::test]
async fn srv_root_lists_posted_handles_over_ap() {
    let srv = SrvFs::new();
    srv.post("agent-runtime", memfs(), Access::ReadWrite).await;

    srv.walk(Fid::ROOT, Fid(1), &[]).await.unwrap();
    srv.open(Fid(1), OpenMode::Read).await.unwrap();
    let listing = String::from_utf8(srv.read(Fid(1), 0, 1024).await.unwrap()).unwrap();
    assert_eq!(listing.lines().collect::<Vec<_>>(), vec!["agent-runtime"]);
}
