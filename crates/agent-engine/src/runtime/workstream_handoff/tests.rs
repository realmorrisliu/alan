use std::path::PathBuf;
use std::sync::Arc;

use alan_agent_protocol::{
    ProcessNamespaceAccess, ProcessNamespaceMount, SpawnHostMount, SpawnMountAccess,
};
use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_routefs::RouteFs;

use crate::runtime::project_workstreams::{
    ProjectLedger, WorkstreamPersistence, WorkstreamRecord, WorkstreamState, WorkstreamWriteMode,
};

use super::{
    HandoffDisposition, MAX_HANDOFF_HOPS, ROUTE_DEAD_LETTER_PORT, ROUTE_LOOP_GUARD_RULE,
    ROUTE_MOUNT_PATH, ROUTE_ROOT_PORT, RouteHandoffKind, RouteHandoffMessage, RoutingReceiptKind,
    WorkstreamAuthorityProfile, WorkstreamHandoffCoordinator, read_route_audit_log,
    send_handoff_message,
};

fn mounted_routefs() -> InProcessTransport {
    let routefs = Arc::new(RouteFs::new());
    let mut ns = Namespace::new();
    ns.mount(
        ROUTE_MOUNT_PATH,
        InProcessTransport::new(routefs),
        Access::ReadWrite,
    );
    InProcessTransport::new(Arc::new(MountFs::new(ns)))
}

fn sample_ledger() -> ProjectLedger {
    let mut ledger = ProjectLedger::empty("proj-key-1");

    let mut ui =
        WorkstreamRecord::new("ui-polish", "TUI layout", WorkstreamPersistence::Persistent);
    ui.state = WorkstreamState::Running;
    ui.write_mode = WorkstreamWriteMode::SharedQueue;

    let mut auth = WorkstreamRecord::new("auth", "Auth and JWT", WorkstreamPersistence::Persistent);
    auth.state = WorkstreamState::Idle;
    auth.last_checkpoint_ref = Some("/tmp/auth-ckpt.jsonl".to_string());
    auth.write_mode = WorkstreamWriteMode::SharedQueue;

    let mut docs =
        WorkstreamRecord::new("docs", "Documentation", WorkstreamPersistence::Persistent);
    docs.state = WorkstreamState::Idle;
    docs.write_mode = WorkstreamWriteMode::ReadOnly;

    let mut archived = WorkstreamRecord::new("old-fix", "Done", WorkstreamPersistence::OneOff);
    archived.state = WorkstreamState::Archived;

    let mut paused =
        WorkstreamRecord::new("broken-cron", "Cron", WorkstreamPersistence::Persistent);
    paused.state = WorkstreamState::PausedError;

    ledger.streams = vec![archived, auth, broken_or(paused), docs, ui];
    ledger
}

fn broken_or(record: WorkstreamRecord) -> WorkstreamRecord {
    record
}

#[tokio::test]
async fn reroute_request_routes_through_routefs_and_records_audit_log() {
    let transport = mounted_routefs();
    let ledger = sample_ledger();
    let mut coordinator = WorkstreamHandoffCoordinator::new(ROUTE_MOUNT_PATH);
    coordinator.ensure_rules(&transport).await.unwrap();

    let msg = RouteHandoffMessage::new(
        RouteHandoffKind::RerouteRequest,
        "ui-polish",
        "corr-reroute-1",
        "Investigate JWT refresh expiry",
    )
    .with_suggested_target("auth");
    send_handoff_message(&transport, &msg).await.unwrap();

    let parent_mounts = vec![
        ProcessNamespaceMount::new("/mnt/repo", ProcessNamespaceAccess::ReadWrite),
        ProcessNamespaceMount::new("/mnt/docs", ProcessNamespaceAccess::ReadOnly),
    ];
    let dispositions = coordinator
        .poll_and_resolve(&transport, &ledger, |target| {
            WorkstreamAuthorityProfile::validated_against_parent(
                target.name.clone(),
                false,
                target.write_mode,
                &[SpawnHostMount {
                    grant: "grant-repo".to_string(),
                    target: PathBuf::from("/mnt/repo"),
                    access: SpawnMountAccess::ReadWrite,
                }],
                &parent_mounts,
                true,
            )
        })
        .await
        .unwrap();

    assert_eq!(dispositions.len(), 1);
    let HandoffDisposition::DispatchToStream(plan) = &dispositions[0] else {
        panic!("expected DispatchToStream, got {:?}", dispositions[0]);
    };
    assert_eq!(plan.target_stream, "auth");
    assert!(plan.focus_target);
    assert_eq!(plan.next_hop_count, 1);
    assert_eq!(plan.checkpoint_ref.as_deref(), Some("/tmp/auth-ckpt.jsonl"));
    assert_eq!(
        plan.receipt.format_inline(),
        "↳ rerouted from [ui-polish] → [auth]"
    );

    let audit = read_route_audit_log(&transport).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].port, ROUTE_ROOT_PORT);
    assert_eq!(
        audit[0].handoff_message().unwrap().correlation_id,
        "corr-reroute-1"
    );
}

#[tokio::test]
async fn cross_stream_dependency_and_terminal_handoff_result_flow() {
    let transport = mounted_routefs();
    let ledger = sample_ledger();
    let mut coordinator = WorkstreamHandoffCoordinator::new(ROUTE_MOUNT_PATH);
    coordinator.ensure_rules(&transport).await.unwrap();

    let dep = RouteHandoffMessage::new(
        RouteHandoffKind::CrossStreamDependency,
        "ui-polish",
        "corr-dep-1",
        "Need updated auth token schema",
    )
    .with_suggested_target("auth")
    .with_hop_count(MAX_HANDOFF_HOPS);
    send_handoff_message(&transport, &dep).await.unwrap();

    let result = RouteHandoffMessage::new(
        RouteHandoffKind::HandoffResult,
        "auth",
        "corr-dep-1",
        "Updated auth token schema",
    )
    .with_suggested_target(ROUTE_ROOT_PORT)
    .with_hop_count(MAX_HANDOFF_HOPS);
    send_handoff_message(&transport, &result).await.unwrap();

    let parent_mounts = vec![ProcessNamespaceMount::new(
        "/mnt/repo",
        ProcessNamespaceAccess::ReadWrite,
    )];
    let dispositions = coordinator
        .poll_and_resolve(&transport, &ledger, |target| {
            WorkstreamAuthorityProfile::validated_against_parent(
                target.name.clone(),
                false,
                target.write_mode,
                &[SpawnHostMount {
                    grant: "grant-repo".to_string(),
                    target: PathBuf::from("/mnt/repo"),
                    access: SpawnMountAccess::ReadOnly,
                }],
                &parent_mounts,
                true,
            )
        })
        .await
        .unwrap();

    assert_eq!(dispositions.len(), 2);
    match &dispositions[0] {
        HandoffDisposition::DispatchToStream(plan) => {
            assert_eq!(plan.target_stream, "auth");
            assert!(!plan.focus_target);
            assert_eq!(plan.next_hop_count, MAX_HANDOFF_HOPS + 1);
            assert_eq!(
                plan.receipt.format_inline(),
                "↳ cross-stream handoff [ui-polish] → [auth]"
            );
        }
        other => panic!("expected DispatchToStream, got {other:?}"),
    }
    match &dispositions[1] {
        HandoffDisposition::DeliverResultToRoot {
            source_stream,
            target_stream,
            receipt,
            ..
        } => {
            assert_eq!(source_stream, "auth");
            assert_eq!(target_stream.as_deref(), Some(ROUTE_ROOT_PORT));
            assert_eq!(receipt.format_inline(), "↳ handoff result [auth] → [root]");
        }
        other => panic!("expected DeliverResultToRoot, got {other:?}"),
    }
}

#[tokio::test]
async fn hop_count_exceeding_max_dead_letters_and_escalates_to_user() {
    let transport = mounted_routefs();
    let ledger = sample_ledger();
    let mut coordinator = WorkstreamHandoffCoordinator::new(ROUTE_MOUNT_PATH);
    coordinator.ensure_rules(&transport).await.unwrap();

    let overflow = RouteHandoffMessage::new(
        RouteHandoffKind::RerouteRequest,
        "auth",
        "corr-loop-1",
        "Please bounce back to ui-polish",
    )
    .with_suggested_target("ui-polish")
    .with_hop_count(MAX_HANDOFF_HOPS + 1);
    send_handoff_message(&transport, &overflow).await.unwrap();

    let parent_mounts = vec![ProcessNamespaceMount::new(
        "/mnt/repo",
        ProcessNamespaceAccess::ReadWrite,
    )];
    let dispositions = coordinator
        .poll_and_resolve(&transport, &ledger, |target| {
            WorkstreamAuthorityProfile::validated_against_parent(
                target.name.clone(),
                false,
                target.write_mode,
                &[],
                &parent_mounts,
                true,
            )
        })
        .await
        .unwrap();

    assert_eq!(dispositions.len(), 1);
    let HandoffDisposition::EscalateToUser {
        hop_count,
        reason,
        receipt,
        ..
    } = &dispositions[0]
    else {
        panic!("expected EscalateToUser, got {:?}", dispositions[0]);
    };
    assert_eq!(*hop_count, MAX_HANDOFF_HOPS + 1);
    assert!(reason.contains("exceeds max_handoff_hops=2"), "{reason}");
    assert_eq!(receipt.kind, RoutingReceiptKind::LoopEscalation);
    assert_eq!(
        receipt.format_inline(),
        "↳ handoff escalated [auth] → [dead-letter] (hop 3 > max 2)"
    );

    let audit = read_route_audit_log(&transport).await.unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].port, ROUTE_DEAD_LETTER_PORT);
    assert_eq!(audit[0].rule, ROUTE_LOOP_GUARD_RULE);
}

#[tokio::test]
async fn correlation_loop_guard_blocks_repeated_zero_hop_bounces() {
    let transport = mounted_routefs();
    let ledger = sample_ledger();
    let mut coordinator = WorkstreamHandoffCoordinator::new(ROUTE_MOUNT_PATH);
    coordinator.ensure_rules(&transport).await.unwrap();

    for hop in 0..=MAX_HANDOFF_HOPS + 1 {
        let (src, dst) = if hop % 2 == 0 {
            ("ui-polish", "auth")
        } else {
            ("auth", "ui-polish")
        };
        let msg = RouteHandoffMessage::new(
            RouteHandoffKind::RerouteRequest,
            src,
            "corr-spoof",
            format!("bounce {hop}"),
        )
        .with_suggested_target(dst);
        send_handoff_message(&transport, &msg).await.unwrap();
    }

    let parent_mounts = vec![ProcessNamespaceMount::new(
        "/mnt/repo",
        ProcessNamespaceAccess::ReadWrite,
    )];
    let dispositions = coordinator
        .poll_and_resolve(&transport, &ledger, |target| {
            WorkstreamAuthorityProfile::validated_against_parent(
                target.name.clone(),
                false,
                target.write_mode,
                &[],
                &parent_mounts,
                true,
            )
        })
        .await
        .unwrap();

    assert_eq!(dispositions.len(), 4);
    assert!(matches!(
        dispositions[0],
        HandoffDisposition::DispatchToStream(_)
    ));
    assert!(matches!(
        dispositions[1],
        HandoffDisposition::DispatchToStream(_)
    ));
    assert!(matches!(
        dispositions[2],
        HandoffDisposition::DispatchToStream(_)
    ));
    assert!(matches!(
        dispositions[3],
        HandoffDisposition::EscalateToUser {
            receipt: super::RoutingReceipt {
                kind: RoutingReceiptKind::LoopEscalation,
                ..
            },
            ..
        }
    ));
}

#[tokio::test]
async fn cross_stream_handoff_never_widens_authority_or_inherits_source_mounts() {
    let parent_mounts = vec![
        ProcessNamespaceMount::new("/mnt/docs", ProcessNamespaceAccess::ReadOnly),
        ProcessNamespaceMount::new("/mnt/repo", ProcessNamespaceAccess::ReadWrite),
    ];

    // ReadOnly target stream cannot widen a ReadOnly mount to ReadWrite.
    let err = WorkstreamAuthorityProfile::validated_against_parent(
        "docs",
        true,
        WorkstreamWriteMode::ReadOnly,
        &[SpawnHostMount {
            grant: "grant-docs".to_string(),
            target: PathBuf::from("/mnt/docs"),
            access: SpawnMountAccess::ReadWrite,
        }],
        &parent_mounts,
        true,
    )
    .unwrap_err();
    assert!(err.contains("read-only workstream `docs`"), "{err}");

    // Target stream cannot widen parent's ReadOnly mount even when stream is writable.
    let err = WorkstreamAuthorityProfile::validated_against_parent(
        "auth",
        false,
        WorkstreamWriteMode::SharedQueue,
        &[SpawnHostMount {
            grant: "grant-docs".to_string(),
            target: PathBuf::from("/mnt/docs"),
            access: SpawnMountAccess::ReadWrite,
        }],
        &parent_mounts,
        true,
    )
    .unwrap_err();
    assert!(err.contains("cannot widen read_only grant"), "{err}");

    // Target stream cannot inherit an ungranted mount (e.g. source stream's /mnt/secrets).
    let err = WorkstreamAuthorityProfile::validated_against_parent(
        "docs",
        false,
        WorkstreamWriteMode::SharedQueue,
        &[SpawnHostMount {
            grant: "grant-secrets".to_string(),
            target: PathBuf::from("/mnt/secrets"),
            access: SpawnMountAccess::ReadOnly,
        }],
        &parent_mounts,
        true,
    )
    .unwrap_err();
    assert!(err.contains("has no live parent grant"), "{err}");

    // Valid target authority carries ONLY target's own validated mounts and fails on exhausted budget.
    let transport = mounted_routefs();
    let ledger = sample_ledger();
    let mut coordinator = WorkstreamHandoffCoordinator::new(ROUTE_MOUNT_PATH);
    coordinator.ensure_rules(&transport).await.unwrap();

    let msg = RouteHandoffMessage::new(
        RouteHandoffKind::CrossStreamDependency,
        "ui-polish",
        "corr-auth-isolation",
        "read docs only",
    )
    .with_suggested_target("docs");
    send_handoff_message(&transport, &msg).await.unwrap();

    let dispositions = coordinator
        .poll_and_resolve(&transport, &ledger, |target| {
            WorkstreamAuthorityProfile::validated_against_parent(
                target.name.clone(),
                false,
                target.write_mode,
                &[SpawnHostMount {
                    grant: "grant-docs".to_string(),
                    target: PathBuf::from("/mnt/docs"),
                    access: SpawnMountAccess::ReadOnly,
                }],
                &parent_mounts,
                false, // target budget exhausted
            )
        })
        .await
        .unwrap();

    assert_eq!(dispositions.len(), 1);
    let HandoffDisposition::EscalateToUser {
        reason, receipt, ..
    } = &dispositions[0]
    else {
        panic!("expected EscalateToUser, got {:?}", dispositions[0]);
    };
    assert!(reason.contains("task_budget"), "{reason}");
    assert_eq!(receipt.kind, RoutingReceiptKind::TargetUnavailable);
}

#[tokio::test]
async fn archived_paused_self_and_missing_targets_escalate_without_dispatch() {
    let transport = mounted_routefs();
    let ledger = sample_ledger();
    let mut coordinator = WorkstreamHandoffCoordinator::new(ROUTE_MOUNT_PATH);
    coordinator.ensure_rules(&transport).await.unwrap();

    for (idx, target) in [
        Some("old-fix"),
        Some("broken-cron"),
        Some("ui-polish"),
        Some("unknown-stream"),
        None,
    ]
    .into_iter()
    .enumerate()
    {
        let mut msg = RouteHandoffMessage::new(
            RouteHandoffKind::RerouteRequest,
            "ui-polish",
            format!("corr-invalid-{idx}"),
            "cannot route",
        );
        if let Some(target) = target {
            msg = msg.with_suggested_target(target);
        }
        send_handoff_message(&transport, &msg).await.unwrap();
    }

    let dispositions = coordinator
        .poll_and_resolve(&transport, &ledger, |target| {
            Ok(WorkstreamAuthorityProfile {
                stream_name: target.name.clone(),
                read_only: true,
                write_mode: target.write_mode,
                host_mounts: Vec::new(),
                budget_available: true,
            })
        })
        .await
        .unwrap();

    assert_eq!(dispositions.len(), 5);
    assert!(
        dispositions
            .iter()
            .all(|d| matches!(d, HandoffDisposition::EscalateToUser { .. }))
    );
}
