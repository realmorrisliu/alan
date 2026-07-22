//! Kernel bootstrap (§7.3): the boot root contains only `/proc`, `/srv`, and the
//! namespace engine. User-space init / Service Manager mounts every higher-level
//! tree later.

use alan_ap::{Fid, OpenMode, Request, Response};
use alan_kernel::{Access, KernelRoot};

async fn read_path(root: &KernelRoot, fid: Fid, names: &[&str]) -> String {
    let transport = root.transport();
    let names = names.iter().map(|name| name.to_string()).collect();
    transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names,
        })
        .await
        .unwrap();
    transport
        .call(Request::Open {
            fid,
            mode: alan_ap::OpenMode::Read,
        })
        .await
        .unwrap();
    match transport
        .call(Request::Read {
            fid,
            offset: 0,
            count: 1024,
        })
        .await
        .unwrap()
    {
        Response::Read { data } => String::from_utf8(data).unwrap(),
        other => panic!("unexpected response: {other:?}"),
    }
}

async fn spawn_at_boot_root(root: &KernelRoot, fid: Fid, exec: &str) -> String {
    let transport = root.transport();
    transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: vec!["proc".into(), "clone".into()],
        })
        .await
        .unwrap();
    transport
        .call(Request::Open {
            fid,
            mode: OpenMode::ReadWrite,
        })
        .await
        .unwrap();
    let pid = match transport
        .call(Request::Read {
            fid,
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    {
        Response::Read { data } => String::from_utf8(data).unwrap(),
        other => panic!("unexpected response: {other:?}"),
    };
    transport
        .call(Request::Write {
            fid,
            offset: 0,
            data: exec.as_bytes().to_vec(),
        })
        .await
        .unwrap();
    transport.call(Request::Clunk { fid }).await.unwrap();
    pid
}

#[tokio::test]
async fn kernel_boot_root_contains_only_proc_and_srv() {
    let root = KernelRoot::new();

    let listing = read_path(&root, Fid(1), &[]).await;
    let mut names = listing.lines().collect::<Vec<_>>();
    names.sort();
    assert_eq!(names, vec!["proc", "srv"]);

    let proc_listing = read_path(&root, Fid(2), &["proc"]).await;
    assert_eq!(proc_listing.lines().collect::<Vec<_>>(), vec!["clone"]);

    let srv_listing = read_path(&root, Fid(3), &["srv"]).await;
    assert_eq!(srv_listing, "");
}

#[tokio::test]
async fn boot_proc_clone_uses_the_boot_namespace_context() {
    let root = KernelRoot::new();
    let exec = serde_json::json!({
        "executable": "/bin/init",
        "args": [],
        "namespace": {
            "generation": 0,
            "mounts": [
                {"path": "/proc", "access": "rw"},
                {"path": "/srv", "access": "rw"}
            ]
        }
    })
    .to_string();

    let pid = spawn_at_boot_root(&root, Fid(20), &exec).await;

    let namespace = read_path(&root, Fid(21), &["proc", &pid, "namespace"]).await;
    assert!(
        namespace.lines().any(|line| line == "/proc rw"),
        "boot-spawned process must inherit /proc: {namespace:?}"
    );
    assert!(
        namespace.lines().any(|line| line == "/srv rw"),
        "boot-spawned process must inherit /srv: {namespace:?}"
    );
}

#[tokio::test]
async fn user_space_mounts_later_trees_through_srv_handles_not_kernel_boot() {
    let root = KernelRoot::new();
    root.srvfs()
        .post(
            "agent",
            alan_ap::InProcessTransport::new(std::sync::Arc::new(alan_ap::reference::MemFs::new())),
            Access::ReadWrite,
        )
        .await;

    let srv_listing = read_path(&root, Fid(10), &["srv"]).await;
    assert_eq!(srv_listing.lines().collect::<Vec<_>>(), vec!["agent"]);

    let root_listing = read_path(&root, Fid(11), &[]).await;
    assert!(
        !root_listing.lines().any(|line| line == "agent"),
        "posting a handle in /srv must not mount the higher-level tree into the kernel boot root"
    );
}
