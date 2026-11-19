//! Coalesced, owner-pinned model/Skill reads never wait in the terminal loop.
use super::*;

pub(super) type Request = tokio::sync::watch::Sender<(u64, String)>;

#[derive(Clone, Copy)]
enum Kind {
    Model,
    Skills,
    Usage,
}

pub(super) enum Snapshot {
    Model(Option<alan_agent_protocol::UiModelSnapshot>),
    Skills(Option<alan_agent_protocol::UiSkillSnapshot>),
    Usage(Option<alan_agent_protocol::UiUsageSnapshot>),
}

pub(super) fn start(
    shell: alan_shell::Shell,
    model: bool,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    jobs: &mut tokio::task::JoinSet<()>,
) -> Request {
    start_kind(
        shell,
        if model { Kind::Model } else { Kind::Skills },
        tx,
        jobs,
    )
}

pub(super) fn start_usage(
    shell: alan_shell::Shell,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    jobs: &mut tokio::task::JoinSet<()>,
) -> Request {
    start_kind(shell, Kind::Usage, tx, jobs)
}

fn start_kind(
    shell: alan_shell::Shell,
    kind: Kind,
    tx: tokio::sync::mpsc::Sender<FileBackedEvent>,
    jobs: &mut tokio::task::JoinSet<()>,
) -> Request {
    let (request, mut rx) = tokio::sync::watch::channel((0, String::new()));
    jobs.spawn(async move {
        while rx.changed().await.is_ok() {
            let (revision, owner) = rx.borrow_and_update().clone();
            if owner.is_empty() {
                continue;
            }
            let snapshot = match kind {
                Kind::Model => Snapshot::Model(model::read_model(&shell, &owner).await),
                Kind::Skills => Snapshot::Skills(skills::read_skills(&shell, &owner).await),
                Kind::Usage => Snapshot::Usage(usage::read_usage(&shell, &owner).await),
            };
            if tx
                .send(FileBackedEvent::ObservationRead {
                    revision,
                    owner,
                    snapshot,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });
    request
}

pub(super) fn request(sender: &Request, owner: String) {
    sender.send_modify(|(revision, requested)| {
        *revision += 1;
        *requested = owner;
    });
}

pub(super) fn apply(
    app: &mut FileBackedApp,
    request: &Request,
    revision: u64,
    owner: &str,
    snapshot: Snapshot,
) {
    if request.borrow().0 != revision {
        return;
    }
    match snapshot {
        Snapshot::Model(snapshot) if app.model.owner == owner => {
            app.model.apply(owner, snapshot);
            app.reconcile_model_chooser();
        }
        Snapshot::Skills(snapshot) if app.skills.owner == owner => {
            app.apply_skills(owner, snapshot)
        }
        Snapshot::Usage(snapshot) if app.usage.owner == owner => app.usage.apply(owner, snapshot),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn held_old_walk_times_out_then_latest_owner_reads_without_gate_release() {
        let (shell, root, namespace, pid) = stdio_tests::live_root_agent().await;
        let fault = Arc::new(stdio_tests::FaultingFileServer::new(root));
        namespace.replace_mount(
            "/agent",
            InProcessTransport::new(fault.clone()),
            alan_kernel::Access::ReadWrite,
        );
        let (reached, release) = fault.pause_next_walk_with_suffix("/machine/ui/models");
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut jobs = tokio::task::JoinSet::new();
        let request = start(shell, true, tx, &mut jobs);
        let owner = format!("/agent/{pid}");
        let result = tokio::time::timeout(std::time::Duration::from_secs(8), async {
            request.send_replace((1, "/agent/root".into()));
            reached.await.map_err(|_| "old walk did not reach gate")?;
            super::request(&request, owner.clone());
            loop {
                let event = rx.recv().await.ok_or("observation worker ended")?;
                if let FileBackedEvent::ObservationRead {
                    revision: 2,
                    owner: actual,
                    snapshot,
                } = event
                {
                    return Ok::<_, &str>((actual, matches!(snapshot, Snapshot::Model(Some(_)))));
                }
            }
        })
        .await;
        // No assertions run until both normal and failed phases have awaited cleanup.
        drop(request);
        if result.is_err() {
            jobs.abort_all();
        }
        let cleanup = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while jobs.join_next().await.is_some() {}
        })
        .await;
        if cleanup.is_err() {
            jobs.abort_all();
            while jobs.join_next().await.is_some() {}
        }
        let (actual, readable) = result
            .expect("latest owner cannot be starved by held old walk")
            .expect("observation completed");
        cleanup.expect("worker cleanup bounded");
        assert_eq!(actual, owner);
        assert!(readable, "latest owner actually read");
        assert!(
            release.send(()).is_err(),
            "old walk cancelled without releasing gate"
        );
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while fault
                .open_paths()
                .iter()
                .any(|p| p.ends_with("/machine/ui/models"))
            {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("read and backing walk fid cleanup");
    }
}
