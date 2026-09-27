use super::*;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[tokio::test]
async fn promotion_lock_wait_is_cancelled_before_writing() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
    let now = Utc::now();
    let inbox_path = stage_inbox_entry(
        &memory_dir,
        InboxEntryDraft {
            kind: "user_preference",
            target: MEMORY_USER_FILENAME.to_string(),
            confidence: "high",
            observation: "Do not write after promotion is cancelled.".to_string(),
            evidence: vec!["Cancellation must win while waiting for the store lock.".to_string()],
            promotion_rationale: "Cancellation contract regression test.".to_string(),
            source_processes: vec!["session-cancelled".to_string()],
        },
        now,
    )
    .await
    .unwrap();

    let held_lock = acquire_promotion_lock(
        &memory_dir.join(MEMORY_USER_FILENAME),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    let memory_dir_for_task = memory_dir.clone();
    let inbox_path_for_task = inbox_path.clone();
    let mut promotion = tokio::spawn(async move {
        promote_inbox_entry(
            &memory_dir_for_task,
            &inbox_path_for_task,
            now,
            &cancel_for_task,
        )
        .await
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    let completed_while_locked =
        tokio::time::timeout(std::time::Duration::from_secs(1), &mut promotion).await;
    drop(held_lock);

    let result = match completed_while_locked {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => panic!("promotion task failed: {error}"),
        Err(_) => {
            let _ = promotion.await;
            panic!("promotion did not observe cancellation while waiting for the lock");
        }
    };
    assert!(result.is_err());
    let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
        .await
        .unwrap();
    assert!(!user_memory.contains("Do not write after promotion is cancelled"));
    let inbox = tokio::fs::read_to_string(inbox_path).await.unwrap();
    assert_eq!(
        parse_inbox_entry(&inbox).unwrap().frontmatter.status,
        "observed"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn write_text_file_does_not_replace_a_read_only_target() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("USER.md");
    tokio::fs::write(&path, "protected memory").await.unwrap();
    let mut permissions = tokio::fs::metadata(&path).await.unwrap().permissions();
    permissions.set_mode(0o444);
    tokio::fs::set_permissions(&path, permissions)
        .await
        .unwrap();

    assert!(write_text_file(&path, "replacement").await.is_err());
    assert_eq!(
        tokio::fs::read_to_string(path).await.unwrap(),
        "protected memory"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn promotion_succeeds_when_memory_store_root_is_not_writable() {
    let temp = TempDir::new().unwrap();
    let memory_dir = temp.path().join("memory-store");
    ensure_memory_store_layout_at(&memory_dir).unwrap();
    let now = Utc::now();
    let inbox_path = stage_inbox_entry(
        &memory_dir,
        InboxEntryDraft {
            kind: "user_preference",
            target: MEMORY_USER_FILENAME.to_string(),
            confidence: "high",
            observation: "Promotion must not create a lock file in the store root.".to_string(),
            evidence: vec!["The target file itself is writable.".to_string()],
            promotion_rationale: "Root directory write access is not required.".to_string(),
            source_processes: vec!["session-read-only-root".to_string()],
        },
        now,
    )
    .await
    .unwrap();
    let original_permissions = tokio::fs::metadata(&memory_dir)
        .await
        .unwrap()
        .permissions();
    let mut read_only_directory = original_permissions.clone();
    read_only_directory.set_mode(0o555);
    tokio::fs::set_permissions(&memory_dir, read_only_directory)
        .await
        .unwrap();

    let result =
        promote_inbox_entry(&memory_dir, &inbox_path, now, &CancellationToken::new()).await;
    tokio::fs::set_permissions(&memory_dir, original_permissions)
        .await
        .unwrap();

    result.unwrap();
    let user_memory = tokio::fs::read_to_string(memory_dir.join(MEMORY_USER_FILENAME))
        .await
        .unwrap();
    assert!(user_memory.contains("Promotion must not create a lock file"));
    assert!(!memory_dir.join(".memory-promotion.lock").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn write_text_file_updates_existing_target_without_parent_write_access() {
    let temp = TempDir::new().unwrap();
    let parent = temp.path().join("topics");
    tokio::fs::create_dir(&parent).await.unwrap();
    let path = parent.join("existing.md");
    tokio::fs::write(&path, "old topic").await.unwrap();
    let inode = tokio::fs::metadata(&path).await.unwrap().ino();
    let original_permissions = tokio::fs::metadata(&parent).await.unwrap().permissions();
    let mut read_only_directory = original_permissions.clone();
    read_only_directory.set_mode(0o555);
    tokio::fs::set_permissions(&parent, read_only_directory)
        .await
        .unwrap();

    let result = write_text_file(&path, "updated topic").await;
    tokio::fs::set_permissions(&parent, original_permissions)
        .await
        .unwrap();

    result.unwrap();
    assert_eq!(
        tokio::fs::read_to_string(&path).await.unwrap(),
        "updated topic"
    );
    assert_eq!(tokio::fs::metadata(path).await.unwrap().ino(), inode);
}
