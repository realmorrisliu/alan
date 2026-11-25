use std::sync::Arc;

use alan_ap::InProcessTransport;
use alan_kernel::{Access, MountFs, Namespace};
use alan_memfs::MemFs;
use tempfile::TempDir;

use super::*;
use crate::runtime::memory_surfaces::project_summary::{
    ProjectSummaryEdit, delete_project_summary, edit_project_summary, inspect_project_summary,
    rollback_project_summary,
};

#[test]
fn apply_memory_proposals_scrubs_deduplicates_and_handles_supersedes() {
    let mut existing = Vec::new();
    let batch1 = vec![
        MemoryProposal {
            kind: MemoryProposalKind::Decision,
            summary: "Use single-writer memfs with key sk-proj-secret1234567890".to_string(),
            provenance_rollout: "rollout://child-1".to_string(),
            supersedes: None,
        },
        MemoryProposal {
            kind: MemoryProposalKind::Decision,
            summary: "  use   single-writer memfs with key sk-proj-secret1234567890  ".to_string(),
            provenance_rollout: "rollout://child-1".to_string(),
            supersedes: None,
        },
        MemoryProposal {
            kind: MemoryProposalKind::Pitfall,
            summary: "   ".to_string(),
            provenance_rollout: "rollout://child-1".to_string(),
            supersedes: None,
        },
    ];

    let out1 = apply_memory_proposals(&mut existing, &batch1);
    assert_eq!(out1.committed.len(), 1);
    assert_eq!(out1.skipped_duplicates, 1);
    assert_eq!(out1.rejected_count, 1);
    assert!(
        !out1.committed[0]
            .summary
            .contains("sk-proj-secret1234567890")
    );
    assert!(out1.committed[0].summary.contains("[REDACTED]"));
    let first_id = out1.committed[0].id.clone();

    let batch2 = vec![MemoryProposal {
        kind: MemoryProposalKind::Decision,
        summary: "Use single-writer memfs with Root atomic commits".to_string(),
        provenance_rollout: "rollout://child-2".to_string(),
        supersedes: Some(first_id),
    }];
    let out2 = apply_memory_proposals(&mut existing, &batch2);
    assert_eq!(out2.committed.len(), 1);
    assert_eq!(out2.superseded_count, 1);
    assert_eq!(existing.len(), 1);
    assert_eq!(
        existing[0].summary,
        "Use single-writer memfs with Root atomic commits"
    );
    assert_eq!(existing[0].provenance_rollout, "rollout://child-2");
}

#[tokio::test]
async fn root_commits_proposals_to_memfs_searchable_via_ro_child_and_supports_summary_rollback() {
    let memfs = Arc::new(MemFs::new());
    let ro_memfs: Arc<dyn alan_ap::FileServer> = Arc::new(memfs.read_only());

    let mut root_ns = Namespace::new();
    root_ns.mount(
        "/mnt/mem",
        InProcessTransport::new(Arc::clone(&memfs) as Arc<dyn alan_ap::FileServer>),
        Access::ReadWrite,
    );
    let root_transport = InProcessTransport::new(Arc::new(MountFs::new(root_ns)));

    let mut child_ns = Namespace::new();
    child_ns.mount(
        "/mnt/mem",
        InProcessTransport::new(ro_memfs),
        Access::ReadOnly,
    );
    let child_transport = InProcessTransport::new(Arc::new(MountFs::new(child_ns)));

    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");

    let proposals = vec![MemoryProposal {
        kind: MemoryProposalKind::Convention,
        summary: "All project workstream state lives in System Store continuity".to_string(),
        provenance_rollout: "rollout://stream-auth-1".to_string(),
        supersedes: None,
    }];

    let commit = commit_memory_proposals_over_ap(
        &root_transport,
        "/mnt/mem",
        &proposals,
        Some(&memory_dir),
        Some("/mnt/proj"),
    )
    .await
    .unwrap();
    assert_eq!(commit.committed.len(), 1);
    assert_eq!(
        commit.memfs_file.as_deref(),
        Some("/mnt/mem/project-memory.md")
    );

    // Child can query `/mnt/mem/recall/<query>` and sees committed proposal + provenance.
    let recall_bytes = try_read_ap_file(&child_transport, "/mnt/mem/recall/workstream")
        .await
        .unwrap()
        .expect("recall hits");
    let recall_text = String::from_utf8(recall_bytes).unwrap();
    assert!(recall_text.contains("project-memory.md"), "{recall_text}");

    // Child direct write attempt to `/mnt/mem` is denied.
    assert!(
        write_or_create_ap_file(&child_transport, "/mnt/mem/rogue.md", b"nope")
            .await
            .is_err()
    );

    // /summary reflects the committed proposal and supports edit, rollback, and delete.
    let view1 = inspect_project_summary(&memory_dir, Some("/mnt/proj")).unwrap();
    let rev1 = view1.summary.as_ref().unwrap().revision;
    assert_eq!(rev1, 1);
    assert!(
        view1
            .summary
            .as_ref()
            .unwrap()
            .source_rollouts
            .contains(&"rollout://stream-auth-1".to_string())
    );

    let edited = edit_project_summary(
        &memory_dir,
        Some("/mnt/proj"),
        Some(rev1),
        ProjectSummaryEdit {
            goal: Some("Curated user goal".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(edited.revision, 2);

    let rolled_back =
        rollback_project_summary(&memory_dir, Some("/mnt/proj"), Some(1), Some(2)).unwrap();
    assert_eq!(rolled_back.revision, 3);
    assert_ne!(rolled_back.goal, "Curated user goal");

    let deleted = delete_project_summary(&memory_dir, Some("/mnt/proj"), Some(3)).unwrap();
    assert!(deleted.deleted);

    // Rolling back a delete restores the pre-delete revision cleanly.
    let restored = rollback_project_summary(
        &memory_dir,
        Some("/mnt/proj"),
        Some(2),
        Some(deleted.revision),
    )
    .unwrap();
    assert!(!restored.deleted);
    assert_eq!(restored.goal, "Curated user goal");
}
