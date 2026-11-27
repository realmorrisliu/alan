//! Integration tests for mount-time `AgentActorView` enforcement (`Client`,
//! `Engine { self_pid }`, `Child { self_pid }`) across `AgentRootFs` and `AgentFs`.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use alan_agentfs::{AgentActorView, AgentFs, AgentRootFs};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, ProcessEventSource,
    Request, Response, Stat,
};
use alan_kernel::{Access, Credentials, MountFs, Namespace, Pid, ProcFs};
use alan_shell::Shell;

static NEXT_TEST_FID: AtomicU64 = AtomicU64::new(100);

fn next_fid() -> Fid {
    Fid(NEXT_TEST_FID.fetch_add(1, Ordering::Relaxed))
}

fn split_path(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|segment| !segment.is_empty())
        .map(str::to_string)
        .collect()
}

async fn walk_path(transport: &InProcessTransport, path: &str) -> Result<Fid, ErrorCode> {
    let fid = next_fid();
    transport
        .call(Request::Walk {
            fid: Fid::ROOT,
            newfid: fid,
            names: split_path(path),
        })
        .await?;
    Ok(fid)
}

async fn stat_path(transport: &InProcessTransport, path: &str) -> Stat {
    let fid = walk_path(transport, path)
        .await
        .unwrap_or_else(|e| panic!("walk {path} failed: {e:?}"));
    let Response::Stat { stat } = transport
        .call(Request::Stat { fid })
        .await
        .unwrap_or_else(|e| panic!("stat {path} failed: {e:?}"))
    else {
        panic!("expected Stat response for {path}");
    };
    let _ = transport.call(Request::Clunk { fid }).await;
    stat
}

async fn assert_write_denied_and_not_writable(transport: &InProcessTransport, path: &str) {
    let stat = stat_path(transport, path).await;
    assert!(
        !stat.writable,
        "{path} must report stat.writable == false, got true"
    );
    for mode in [
        OpenMode::Write,
        OpenMode::WriteExclusive,
        OpenMode::ReadWrite,
    ] {
        let fid = walk_path(transport, path)
            .await
            .unwrap_or_else(|e| panic!("walk {path} for {mode:?} failed: {e:?}"));
        let err = transport
            .call(Request::Open { fid, mode })
            .await
            .expect_err(&format!("{path} open({mode:?}) must fail with NoAccess"));
        assert_eq!(
            err,
            ErrorCode::NoAccess,
            "{path} open({mode:?}) returned {err:?} instead of NoAccess"
        );
        let _ = transport.call(Request::Clunk { fid }).await;
    }
}

async fn assert_write_allowed(
    transport: &InProcessTransport,
    path: &str,
    payload: &[u8],
) -> Result<(), ErrorCode> {
    let stat = stat_path(transport, path).await;
    assert!(
        stat.writable,
        "{path} must report stat.writable == true, got false"
    );
    let fid = walk_path(transport, path).await?;
    transport
        .call(Request::Open {
            fid,
            mode: OpenMode::Write,
        })
        .await?;
    transport
        .call(Request::Write {
            fid,
            offset: 0,
            data: payload.to_vec(),
        })
        .await?;
    transport.call(Request::Clunk { fid }).await?;
    Ok(())
}

async fn clone_entry(transport: &InProcessTransport, clone_path: &str) -> String {
    let fid = walk_path(transport, clone_path).await.unwrap();
    transport
        .call(Request::Open {
            fid,
            mode: OpenMode::ReadWrite,
        })
        .await
        .unwrap();
    let Response::Read { data } = transport
        .call(Request::Read {
            fid,
            offset: 0,
            count: 64,
        })
        .await
        .unwrap()
    else {
        panic!("expected Read response");
    };
    transport.call(Request::Clunk { fid }).await.unwrap();
    String::from_utf8(data).unwrap()
}

struct Fixture {
    agent_root: Arc<AgentRootFs>,
    root_pid: String,
    child_pid: String,
    sibling_pid: String,
    root_request_id: String,
    child_request_id: String,
    root_action_id: String,
}

async fn setup_fixture() -> Fixture {
    let procfs = Arc::new(ProcFs::new());
    let proc_server: Arc<dyn FileServer> = procfs.clone();
    let proc_events: Arc<dyn ProcessEventSource> = procfs.clone();
    let agent_root = Arc::new(AgentRootFs::new_with_process_events(
        proc_server,
        proc_events,
    ));

    // Attach a mock /agent/history subtree exposing catalog (ro), ctl (rw), summary (ro), summary_ctl (rw).
    let history_fs = Arc::new(alan_ap::reference::MemFs::writable());
    for (name, body) in [
        ("catalog", b"[]".as_slice()),
        ("ctl", b"".as_slice()),
        ("summary", b"{}".as_slice()),
        ("summary_ctl", b"".as_slice()),
    ] {
        let fid = next_fid();
        history_fs
            .create(Fid::ROOT, fid, name, FileKind::File)
            .await
            .unwrap();
        history_fs.open(fid, OpenMode::Write).await.unwrap();
        history_fs.write(fid, 0, body).await.unwrap();
        history_fs.clunk(fid).await.unwrap();
    }
    agent_root.bind_history_server(history_fs).await;

    let spawn_with_parent = |parent: Option<Pid>| {
        let procfs = procfs.clone();
        async move {
            let spawner = procfs.for_spawner(parent, Namespace::new(), Credentials::user("alan"));
            let fid = next_fid();
            spawner
                .walk(Fid::ROOT, fid, &["clone".to_string()])
                .await
                .unwrap();
            spawner.open(fid, OpenMode::ReadWrite).await.unwrap();
            let pid = String::from_utf8(spawner.read(fid, 0, 64).await.unwrap()).unwrap();
            spawner
                .write(
                    fid,
                    0,
                    br#"{"executable":"/bin/alan-agent","args":[],"namespace":{"generation":0,"mounts":[]}}"#,
                )
                .await
                .unwrap();
            spawner.clunk(fid).await.unwrap();
            pid
        }
    };

    let root_pid = spawn_with_parent(None).await;
    let root_pid_num = Pid(root_pid.parse().unwrap());
    let child_pid = spawn_with_parent(Some(root_pid_num)).await;
    let sibling_pid = spawn_with_parent(Some(root_pid_num)).await;

    let root_agent_fs = Arc::new(AgentFs::new());
    let child_agent_fs = Arc::new(AgentFs::new());
    let sibling_agent_fs = Arc::new(AgentFs::new());

    agent_root
        .bind_process(root_pid.clone(), root_agent_fs.clone())
        .await;
    agent_root.set_root_process(root_pid.clone()).await;
    agent_root
        .bind_process(child_pid.clone(), child_agent_fs.clone())
        .await;
    agent_root
        .bind_process(sibling_pid.clone(), sibling_agent_fs.clone())
        .await;

    // Seed one pending request and one action on root, and one pending request on child,
    // via each agent's own Engine view.
    let root_engine = mount_agent_view(
        &agent_root,
        AgentActorView::Engine {
            self_pid: root_pid.clone(),
        },
    );
    let root_request_id =
        clone_entry(&root_engine, &format!("/agent/{root_pid}/requests/clone")).await;
    assert_write_allowed(
        &root_engine,
        &format!("/agent/{root_pid}/requests/{root_request_id}/prompt"),
        b"approve?",
    )
    .await
    .unwrap();
    let root_action_id =
        clone_entry(&root_engine, &format!("/agent/{root_pid}/actions/clone")).await;
    assert_write_allowed(
        &root_engine,
        &format!("/agent/{root_pid}/actions/{root_action_id}/name"),
        b"read_file",
    )
    .await
    .unwrap();

    let child_engine = mount_agent_view(
        &agent_root,
        AgentActorView::Child {
            self_pid: child_pid.clone(),
        },
    );
    let child_request_id =
        clone_entry(&child_engine, &format!("/agent/{child_pid}/requests/clone")).await;

    Fixture {
        agent_root,
        root_pid,
        child_pid,
        sibling_pid,
        root_request_id,
        child_request_id,
        root_action_id,
    }
}

fn mount_agent_view(agent_root: &Arc<AgentRootFs>, view: AgentActorView) -> InProcessTransport {
    let mut namespace = Namespace::new();
    namespace.mount(
        "/agent",
        InProcessTransport::new(agent_root.actor_view(view)),
        Access::ReadWrite,
    );
    InProcessTransport::new(Arc::new(MountFs::new(namespace)))
}

#[tokio::test]
async fn client_view_rejects_engine_surfaces_and_allows_client_surfaces() {
    let fx = setup_fixture().await;
    let client = mount_agent_view(&fx.agent_root, AgentActorView::Client);
    let shell = Shell::new(client.clone());

    // Check both `/agent/root` alias and concrete `/agent/<root_pid>`.
    for prefix in ["/agent/root".to_string(), format!("/agent/{}", fx.root_pid)] {
        let engine_owned = vec![
            format!("{prefix}/io/output"),
            format!("{prefix}/io/events"),
            format!("{prefix}/ctl"),
            format!("{prefix}/machine/tape"),
            format!("{prefix}/machine/status"),
            format!("{prefix}/machine/evaluation"),
            format!("{prefix}/machine/work"),
            format!("{prefix}/machine/ui/activity"),
            format!("{prefix}/machine/ui/events"),
            format!("{prefix}/machine/ui/models"),
            format!("{prefix}/machine/ui/queue"),
            format!("{prefix}/machine/ui/plan"),
            format!("{prefix}/machine/ui/thinking"),
            format!("{prefix}/machine/ui/notice"),
            format!("{prefix}/machine/ui/usage"),
            format!("{prefix}/machine/ui/budget"),
            format!("{prefix}/machine/ui/skills"),
            format!("{prefix}/machine/ui/system1"),
            format!("{prefix}/requests/clone"),
            format!("{prefix}/requests/{}/kind", fx.root_request_id),
            format!("{prefix}/requests/{}/prompt", fx.root_request_id),
            format!("{prefix}/requests/{}/options", fx.root_request_id),
            format!("{prefix}/requests/{}/status", fx.root_request_id),
            format!("{prefix}/requests/{}/ctl", fx.root_request_id),
            format!("{prefix}/actions/clone"),
            format!("{prefix}/actions/{}/name", fx.root_action_id),
            format!("{prefix}/actions/{}/status", fx.root_action_id),
            format!("{prefix}/actions/{}/output", fx.root_action_id),
            format!("{prefix}/actions/{}/result", fx.root_action_id),
            format!("{prefix}/actions/{}/approval", fx.root_action_id),
            format!("{prefix}/actions/{}/process", fx.root_action_id),
        ];
        for path in engine_owned {
            assert_write_denied_and_not_writable(&client, &path).await;
        }
    }

    // Directories are also non-writable.
    for dir in [
        "/agent".to_string(),
        "/agent/root".to_string(),
        format!("/agent/{}/children", fx.root_pid),
    ] {
        assert_write_denied_and_not_writable(&client, &dir).await;
    }

    // Reads on engine-owned surfaces remain allowed.
    assert!(shell.cat("/agent/root/machine/ui/activity").await.is_ok());
    assert!(shell.cat("/agent/root/io/output").await.is_ok());
    assert!(shell.cat("/agent/root/machine/tape").await.is_ok());

    // Client-writable surfaces succeed and report stat.writable == true.
    assert_write_allowed(&client, "/agent/root/io/input", b"hello client")
        .await
        .unwrap();
    assert_write_allowed(&client, "/agent/root/machine/ctl", b"interrupt")
        .await
        .unwrap();
    assert_write_allowed(
        &client,
        &format!("/agent/root/requests/{}/response", fx.root_request_id),
        b"approved",
    )
    .await
    .unwrap();
    assert_eq!(
        shell
            .cat(&format!(
                "/agent/root/requests/{}/status",
                fx.root_request_id
            ))
            .await
            .unwrap(),
        b"answered"
    );

    // `/agent/history` continuity controls remain writable for Client; catalog/summary remain ro.
    assert_write_allowed(&client, "/agent/history/ctl", b"fresh")
        .await
        .unwrap();
    assert_write_allowed(&client, "/agent/history/summary_ctl", b"delete")
        .await
        .unwrap();
    assert_write_denied_and_not_writable(&client, "/agent/history/catalog").await;
    assert_write_denied_and_not_writable(&client, "/agent/history/summary").await;
}

#[tokio::test]
async fn client_multi_step_walks_and_fid_clones_preserve_enforcement() {
    let fx = setup_fixture().await;
    let view = fx.agent_root.actor_view(AgentActorView::Client);

    // Multi-step walk: root -> io -> output (denied), root -> io -> input (allowed).
    let root_fid = Fid(1);
    let io_fid = Fid(2);
    let output_fid = Fid(3);
    let output_clone_fid = Fid(4);
    let input_fid = Fid(5);

    view.walk(Fid::ROOT, root_fid, &["root".into()])
        .await
        .unwrap();
    view.walk(root_fid, io_fid, &["io".into()]).await.unwrap();
    view.walk(io_fid, output_fid, &["output".into()])
        .await
        .unwrap();
    view.walk(output_fid, output_clone_fid, &[]).await.unwrap();
    view.walk(io_fid, input_fid, &["input".into()])
        .await
        .unwrap();

    assert!(!view.stat(output_fid).await.unwrap().writable);
    assert!(!view.stat(output_clone_fid).await.unwrap().writable);
    assert_eq!(
        view.open(output_clone_fid, OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        view.open(output_clone_fid, OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );
    assert!(view.stat(input_fid).await.unwrap().writable);
    view.open(input_fid, OpenMode::Write).await.unwrap();
    view.write(input_fid, 0, b"multi-step").await.unwrap();
    for fid in [input_fid, output_clone_fid, output_fid, io_fid, root_fid] {
        view.clunk(fid).await.unwrap();
    }
}

#[tokio::test]
async fn engine_view_scopes_engine_writes_to_self_pid_and_allows_child_client_orchestration() {
    let fx = setup_fixture().await;
    let root_engine = mount_agent_view(
        &fx.agent_root,
        AgentActorView::Engine {
            self_pid: fx.root_pid.clone(),
        },
    );

    // Own pid + `/agent/root` alias allow engine writes.
    for prefix in [format!("/agent/{}", fx.root_pid), "/agent/root".to_string()] {
        assert_write_allowed(&root_engine, &format!("{prefix}/io/output"), b"assistant")
            .await
            .unwrap();
        assert_write_allowed(&root_engine, &format!("{prefix}/machine/tape"), b"record\n")
            .await
            .unwrap();
        assert_write_allowed(
            &root_engine,
            &format!("{prefix}/machine/ui/activity"),
            br#"{"version":1,"state":"running"}"#,
        )
        .await
        .unwrap();
        assert_write_allowed(
            &root_engine,
            &format!("{prefix}/machine/ui/events"),
            b"{}\n",
        )
        .await
        .unwrap();
    }
    // Backing read-only nodes on own pid remain read-only.
    for ro in [
        format!("/agent/{}/machine/status", fx.root_pid),
        format!("/agent/{}/machine/evaluation", fx.root_pid),
        format!("/agent/{}/machine/work", fx.root_pid),
        format!("/agent/{}/io/events", fx.root_pid),
        format!("/agent/{}/events", fx.root_pid),
        format!(
            "/agent/{}/requests/{}/status",
            fx.root_pid, fx.root_request_id
        ),
    ] {
        assert_write_denied_and_not_writable(&root_engine, &ro).await;
    }

    // On child pid (both direct `/agent/<child>` and `/agent/<root>/children/<child>`),
    // engine-owned writes are denied while client-surface orchestration writes succeed.
    for child_prefix in [
        format!("/agent/{}", fx.child_pid),
        format!("/agent/{}/children/{}", fx.root_pid, fx.child_pid),
    ] {
        for forbidden in [
            format!("{child_prefix}/io/output"),
            format!("{child_prefix}/machine/tape"),
            format!("{child_prefix}/machine/ui/activity"),
            format!("{child_prefix}/machine/ui/events"),
            format!("{child_prefix}/requests/clone"),
            format!("{child_prefix}/requests/{}/prompt", fx.child_request_id),
            format!("{child_prefix}/requests/{}/ctl", fx.child_request_id),
            format!("{child_prefix}/actions/clone"),
            format!("{child_prefix}/ctl"),
        ] {
            assert_write_denied_and_not_writable(&root_engine, &forbidden).await;
        }
        assert_write_allowed(&root_engine, &format!("{child_prefix}/io/input"), b"steer")
            .await
            .unwrap();
        assert_write_allowed(
            &root_engine,
            &format!("{child_prefix}/machine/ctl"),
            b"interrupt",
        )
        .await
        .unwrap();
    }
    assert_write_allowed(
        &root_engine,
        &format!(
            "/agent/{}/requests/{}/response",
            fx.child_pid, fx.child_request_id
        ),
        b"approved",
    )
    .await
    .unwrap();

    // Non-root Engine cannot write engine-owned surfaces via `/agent/root`.
    let non_root_engine = mount_agent_view(
        &fx.agent_root,
        AgentActorView::Engine {
            self_pid: fx.sibling_pid.clone(),
        },
    );
    assert_write_denied_and_not_writable(&non_root_engine, "/agent/root/io/output").await;
    assert_write_denied_and_not_writable(&non_root_engine, "/agent/root/machine/tape").await;
    assert_write_denied_and_not_writable(&non_root_engine, "/agent/history/ctl").await;
}

#[tokio::test]
async fn child_view_allows_own_pid_and_keeps_parent_root_and_siblings_read_only() {
    let fx = setup_fixture().await;
    let child_view = mount_agent_view(
        &fx.agent_root,
        AgentActorView::Child {
            self_pid: fx.child_pid.clone(),
        },
    );
    let shell = Shell::new(child_view.clone());

    // Own pid (direct and via parent's children/ overlay) permits engine writes.
    for own_prefix in [
        format!("/agent/{}", fx.child_pid),
        format!("/agent/{}/children/{}", fx.root_pid, fx.child_pid),
    ] {
        assert_write_allowed(
            &child_view,
            &format!("{own_prefix}/io/output"),
            b"child out",
        )
        .await
        .unwrap();
        assert_write_allowed(&child_view, &format!("{own_prefix}/machine/tape"), b"t\n")
            .await
            .unwrap();
        assert_write_allowed(
            &child_view,
            &format!("{own_prefix}/machine/ui/activity"),
            br#"{"version":1,"state":"idle"}"#,
        )
        .await
        .unwrap();
    }

    // Parent, `/agent/root`, sibling, and `/agent/history` are strictly read-only —
    // even client surfaces (`io/input`, `machine/ctl`, `requests/<id>/response`).
    for foreign_prefix in [
        format!("/agent/{}", fx.root_pid),
        "/agent/root".to_string(),
        format!("/agent/{}", fx.sibling_pid),
    ] {
        for path in [
            format!("{foreign_prefix}/io/input"),
            format!("{foreign_prefix}/io/output"),
            format!("{foreign_prefix}/machine/ctl"),
            format!("{foreign_prefix}/machine/tape"),
            format!("{foreign_prefix}/machine/ui/activity"),
            format!("{foreign_prefix}/requests/clone"),
            format!("{foreign_prefix}/actions/clone"),
            format!("{foreign_prefix}/ctl"),
        ] {
            assert_write_denied_and_not_writable(&child_view, &path).await;
        }
    }
    assert_write_denied_and_not_writable(
        &child_view,
        &format!(
            "/agent/{}/requests/{}/response",
            fx.root_pid, fx.root_request_id
        ),
    )
    .await;
    assert_write_denied_and_not_writable(&child_view, "/agent/history/ctl").await;
    assert_write_denied_and_not_writable(&child_view, "/agent/history/summary_ctl").await;

    // Reads on parent and sibling remain permitted.
    assert!(shell.cat("/agent/root/machine/ui/activity").await.is_ok());
    assert!(
        shell
            .cat(&format!("/agent/{}/machine/ui/activity", fx.sibling_pid))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn agent_fs_view_enforces_actor_writability_directly_without_fid_collisions() {
    let fs = Arc::new(AgentFs::new());
    let engine = fs.actor_view(
        "42",
        AgentActorView::Engine {
            self_pid: "42".to_string(),
        },
    );
    let client = fs.actor_view("42", AgentActorView::Client);
    let foreign_child = fs.actor_view(
        "42",
        AgentActorView::Child {
            self_pid: "99".to_string(),
        },
    );

    // Both views use Fid(1) simultaneously without colliding on the underlying AgentFs.
    engine
        .walk(Fid::ROOT, Fid(1), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    client
        .walk(Fid::ROOT, Fid(1), &["machine".into(), "tape".into()])
        .await
        .unwrap();
    foreign_child
        .walk(Fid::ROOT, Fid(1), &["io".into(), "input".into()])
        .await
        .unwrap();

    assert!(engine.stat(Fid(1)).await.unwrap().writable);
    assert!(!client.stat(Fid(1)).await.unwrap().writable);
    assert!(!foreign_child.stat(Fid(1)).await.unwrap().writable);

    assert_eq!(
        client.open(Fid(1), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        client.open(Fid(1), OpenMode::WriteExclusive).await,
        Err(ErrorCode::NoAccess)
    );
    assert_eq!(
        foreign_child.open(Fid(1), OpenMode::Write).await,
        Err(ErrorCode::NoAccess)
    );

    engine.open(Fid(1), OpenMode::Write).await.unwrap();
    engine.write(Fid(1), 0, b"engine-tape\n").await.unwrap();
    engine.clunk(Fid(1)).await.unwrap();

    client.open(Fid(1), OpenMode::Read).await.unwrap();
    assert_eq!(client.read(Fid(1), 0, 64).await.unwrap(), b"engine-tape\n");
    client.clunk(Fid(1)).await.unwrap();
    foreign_child.clunk(Fid(1)).await.unwrap();
}
