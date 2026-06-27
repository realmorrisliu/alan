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
async fn a_withheld_handle_is_filtered_on_the_ap_surface_and_not_remountable() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadWrite).await;
    srv.post("mem", memfs(), Access::ReadOnly).await;

    // A restricted child's /srv is a real filtered file server withholding "llm".
    let denied: HashSet<String> = ["llm".to_string()].into_iter().collect();
    let view = srv.view(&denied).await;

    assert_eq!(
        view.list().await,
        vec!["mem".to_string()],
        "withheld handle absent from listing"
    );
    assert!(
        view.lookup("mem").await.is_some(),
        "permitted handle still mounts"
    );
    assert!(
        view.lookup("llm").await.is_none(),
        "withheld service not resolvable"
    );

    // Crucially, the filter holds on the aP surface the process reads: reading
    // /srv lists only "mem", and walking the withheld handle fails.
    view.walk(Fid::ROOT, Fid(1), &[]).await.unwrap();
    view.open(Fid(1), OpenMode::Read).await.unwrap();
    let listing = String::from_utf8(view.read(Fid(1), 0, 1024).await.unwrap()).unwrap();
    assert_eq!(
        listing.lines().collect::<Vec<_>>(),
        vec!["mem"],
        "aP read is filtered too"
    );
    assert_eq!(
        view.walk(Fid::ROOT, Fid(2), &["llm".into()]).await,
        Err(alan_ap::ErrorCode::NotFound),
        "withheld handle is not walkable over aP"
    );
}

#[tokio::test]
async fn srv_walk_binds_fids_and_handles_get_unique_qids() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadWrite).await;
    srv.post("mem", memfs(), Access::ReadOnly).await;

    // Walking a handle binds the fid to that handle; reading it returns the
    // handle name (not the root listing), and the qids are distinct per handle.
    let llm_qid = srv.walk(Fid::ROOT, Fid(1), &["llm".into()]).await.unwrap();
    let mem_qid = srv.walk(Fid::ROOT, Fid(2), &["mem".into()]).await.unwrap();
    assert_ne!(
        llm_qid.path, mem_qid.path,
        "distinct handles get distinct qids"
    );

    srv.open(Fid(1), OpenMode::Read).await.unwrap();
    let bytes = srv.read(Fid(1), 0, 64).await.unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "llm",
        "the handle fid reads its own entry"
    );
}

#[tokio::test]
async fn reposting_a_name_replaces_the_stale_handle() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadOnly).await;
    // A restart re-posts the same name with new access; it supersedes, not dupes.
    srv.post("llm", memfs(), Access::ReadWrite).await;

    assert_eq!(
        srv.list().await,
        vec!["llm".to_string()],
        "one entry per name"
    );
    let (_tree, access) = srv.lookup("llm").await.unwrap();
    assert_eq!(
        access,
        Access::ReadWrite,
        "lookup returns the current handle, not the stale one"
    );
}

// A filtered view shares the live registry: a later permitted post on the parent
// is visible to the child, and a withheld name stays hidden (PR #574 review).
#[tokio::test]
async fn filtered_view_stays_live() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadWrite).await;

    let denied: HashSet<String> = ["mem".to_string()].into_iter().collect();
    let view = srv.view(&denied).await;
    assert_eq!(view.list().await, vec!["llm".to_string()]);

    // Parent posts a newly permitted handle and a denied one after the view exists.
    srv.post("route", memfs(), Access::ReadWrite).await;
    srv.post("mem", memfs(), Access::ReadOnly).await;

    let mut names = view.list().await;
    names.sort();
    assert_eq!(
        names,
        vec!["llm".to_string(), "route".to_string()],
        "view sees new allowed post, not the denied one"
    );
    assert!(
        view.lookup("route").await.is_some(),
        "newly posted permitted handle resolves"
    );
    assert!(
        view.lookup("mem").await.is_none(),
        "denied handle stays hidden"
    );
}

// stat on a /srv handle reports its readable byte length, not 0 (PR #574 review).
#[tokio::test]
async fn handle_stat_reports_real_length() {
    let srv = SrvFs::new();
    srv.post("llm", memfs(), Access::ReadWrite).await;
    srv.walk(Fid::ROOT, Fid(1), &["llm".into()]).await.unwrap();
    let st = srv.stat(Fid(1)).await.unwrap();
    assert_eq!(
        st.length, 3,
        "handle file length matches the bytes read returns (\"llm\")"
    );
}

// Concurrent walks reusing the same newfid don't both succeed: the fid table is
// reserved atomically, so exactly one binds and the other is rejected (PR #574).
#[tokio::test]
async fn concurrent_walks_on_one_newfid_do_not_clobber() {
    use std::sync::Arc;
    let srv = Arc::new(SrvFs::new());
    srv.post("llm", memfs(), Access::ReadWrite).await;
    srv.post("mem", memfs(), Access::ReadOnly).await;

    let a = srv.clone();
    let b = srv.clone();
    let h1 = tokio::spawn(async move { a.walk(Fid::ROOT, Fid(1), &["llm".into()]).await });
    let h2 = tokio::spawn(async move { b.walk(Fid::ROOT, Fid(1), &["mem".into()]).await });
    let (r1, r2) = (h1.await.unwrap(), h2.await.unwrap());

    // Exactly one walk binds Fid(1); the other is rejected, not a silent rebind.
    assert!(
        r1.is_ok() ^ r2.is_ok(),
        "exactly one of the racing walks binds the shared newfid: {r1:?} {r2:?}"
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
