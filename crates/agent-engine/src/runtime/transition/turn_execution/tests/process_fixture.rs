use alan_ap::{Fid, FileServer, OpenMode};

pub(super) async fn spawn_turn_test_parent(
    procfs: &alan_kernel::ProcFs,
    namespace: &alan_kernel::Namespace,
) {
    let spawner = procfs.for_spawner(
        None,
        namespace.clone(),
        alan_kernel::Credentials::user("root-agent"),
    );
    let fid = Fid(75_000);
    spawner
        .walk(Fid::ROOT, fid, &["clone".to_string()])
        .await
        .unwrap();
    spawner.open(fid, OpenMode::ReadWrite).await.unwrap();
    assert_eq!(
        String::from_utf8(spawner.read(fid, 0, 64).await.unwrap()).unwrap(),
        "1"
    );
    let exec = alan_kernel::ExecSpec {
        executable: "/bin/agent".to_string(),
        args: Vec::new(),
        namespace: alan_kernel::ExecNamespaceManifest::from_namespace(namespace),
        descriptors: Default::default(),
    };
    spawner
        .write(fid, 0, &serde_json::to_vec(&exec).unwrap())
        .await
        .unwrap();
    spawner.clunk(fid).await.unwrap();
}
