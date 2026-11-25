use super::*;
use crate::test_support::tool_context_with_root;
use alan_agent_engine::Config;
use alan_agent_engine::tools::{Tool, ToolContext};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_read_file_tool() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create test file
    tokio::fs::write(mount_root.join("test.txt"), "line1\nline2\nline3\n")
        .await
        .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "test.txt"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["type"], "text");
    assert!(result["content"].as_str().unwrap().contains("line1"));
}

#[tokio::test]
async fn test_read_file_tool_uses_mount_root_binding_from_context() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().join("mount_root");
    tokio::fs::create_dir_all(&mount_root).await.unwrap();
    tokio::fs::write(mount_root.join("test.txt"), "bound\n")
        .await
        .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "test.txt"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["path"], json!("/mnt/source/test.txt"));
    assert_eq!(result["content"], json!("1| bound"));
}

#[tokio::test]
async fn test_read_file_tool_requires_explicit_sandbox_grant() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();
    tokio::fs::write(mount_root.join("test.txt"), "hello\n")
        .await
        .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = ToolContext::from_binding(
        alan_agent_engine::tools::ToolExecutionBinding::awaiting_host_projection(
            PathBuf::from("/mnt/source"),
            mount_root.join("tmp"),
        ),
        config,
    );

    let err = tool
        .execute(json!({"path": "test.txt"}), &ctx)
        .await
        .unwrap_err();

    let message = err.to_string();
    assert!(message.contains("No project directory is authorized"));
    assert!(message.contains("Run `/project` in Alan Shell"));
    assert!(message.contains("Tool Process has no explicit Host execution adapter"));
}

#[tokio::test]
async fn test_read_file_with_offset_and_limit() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(
        mount_root.join("lines.txt"),
        "line1\nline2\nline3\nline4\nline5\n",
    )
    .await
    .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    // Read from line 2, max 2 lines
    let args = json!({"path": "lines.txt", "offset": 2, "limit": 2});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["content"], "2| line2\n3| line3");
    assert_eq!(result["start_line"], 2);
    assert_eq!(result["end_line"], 3);
    assert_eq!(result["total_lines"], 5);
    assert!(result["truncated"].as_bool().unwrap());

    // Read to EOF with offset: must not report truncated=true when end == total_lines
    let eof_args = json!({"path": "lines.txt", "offset": 4, "limit": 2});
    let eof_result = tool.execute(eof_args, &ctx).await.unwrap();
    assert_eq!(eof_result["content"], "4| line4\n5| line5");
    assert_eq!(eof_result["start_line"], 4);
    assert_eq!(eof_result["end_line"], 5);
    assert!(!eof_result["truncated"].as_bool().unwrap());
}

#[tokio::test]
async fn test_read_file_offset_beyond_content() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("short.txt"), "one line")
        .await
        .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "short.txt", "offset": 10});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["content"], "");
    assert_eq!(result["total_lines"], 1);
}

#[tokio::test]
async fn test_read_file_caps_large_multiline_output_inline_for_clean_offset_paging() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // 400 lines * ~100 bytes = ~40KB (> MAX_TEXT_READ_BYTES = 20_000, > MAX_INLINE_EVIDENCE_BYTES = 30_000)
    let mut lines = String::new();
    for idx in 1..=400 {
        lines.push_str(&format!("line-{idx:03} {}\n", "x".repeat(90)));
    }
    tokio::fs::write(mount_root.join("large.txt"), &lines)
        .await
        .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let first = tool
        .execute(json!({"path": "large.txt"}), &ctx)
        .await
        .unwrap();
    assert_eq!(first["start_line"], 1);
    let end_1 = first["end_line"].as_u64().unwrap();
    assert!(end_1 < 400);
    assert_eq!(first["truncated"], true);
    assert!(
        serde_json::to_vec(&first).unwrap().len() < 30_000,
        "first page should stay below evidence projection threshold"
    );

    let second = tool
        .execute(json!({"path": "large.txt", "offset": end_1 + 1}), &ctx)
        .await
        .unwrap();
    assert_eq!(second["start_line"], end_1 + 1);
    let expected_prefix = format!("{}| line-{:03}", end_1 + 1, end_1 + 1);
    assert!(
        second["content"]
            .as_str()
            .unwrap()
            .starts_with(&expected_prefix),
        "second page must start at line {}, got: {}",
        end_1 + 1,
        &second["content"].as_str().unwrap()[..80]
    );
}

#[tokio::test]
async fn test_read_file_not_found() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "nonexistent.txt"});
    let result = tool.execute(args, &ctx).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_read_image_file() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create a fake PNG file (just the header bytes)
    let png_header = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    tokio::fs::write(mount_root.join("test.png"), png_header)
        .await
        .unwrap();

    let tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "test.png"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["type"], "image");
    assert_eq!(result["mime_type"], "image/png");
    assert_eq!(result["size_bytes"], 8);
    let parts: Vec<alan_agent_protocol::ContentPart> =
        serde_json::from_value(result["content_parts"].clone()).unwrap();
    assert_eq!(parts.len(), 1);
    match &parts[0] {
        alan_agent_protocol::ContentPart::Attachment {
            mime_type,
            metadata,
            ..
        } => {
            assert_eq!(mime_type, "image/png");
            assert_eq!(metadata["data"], json!("iVBORw0KGgo="));
        }
        other => panic!("expected attachment part, got {other:?}"),
    }
}

#[tokio::test]
async fn test_write_and_read_file() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let write_tool = WriteFileTool::new();
    let read_tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    // Write
    let write_args = json!({"path": "output.txt", "content": "Hello World"});
    let write_result = write_tool.execute(write_args, &ctx).await.unwrap();
    assert!(write_result["success"].as_bool().unwrap());

    // Read back
    let read_args = json!({"path": "output.txt"});
    let read_result = read_tool.execute(read_args, &ctx).await.unwrap();
    assert_eq!(read_result["content"], "1| Hello World");
}

#[tokio::test]
async fn test_write_file_creates_parent_dirs() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = WriteFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "a/b/c/deep.txt", "content": "deep content"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert!(result["success"].as_bool().unwrap());

    // Verify file exists
    let content = tokio::fs::read_to_string(mount_root.join("a/b/c/deep.txt"))
        .await
        .unwrap();
    assert_eq!(content, "deep content");
}

#[tokio::test]
async fn test_write_file_empty_content() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = WriteFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "empty.txt", "content": ""});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert!(result["success"].as_bool().unwrap());
    assert_eq!(result["bytes_written"], 0);
}

#[tokio::test]
async fn test_write_file_overwrites_existing() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create existing file
    tokio::fs::write(mount_root.join("existing.txt"), "old content")
        .await
        .unwrap();

    let tool = WriteFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "existing.txt", "content": "new content"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert!(result["success"].as_bool().unwrap());

    let content = tokio::fs::read_to_string(mount_root.join("existing.txt"))
        .await
        .unwrap();
    assert_eq!(content, "new content");
}

#[tokio::test]
async fn test_edit_file() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create file
    tokio::fs::write(mount_root.join("edit.txt"), "Hello World")
        .await
        .unwrap();

    let edit_tool = EditFileTool::new();
    let read_tool = ReadFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    // Edit
    let edit_args = json!({"path": "edit.txt", "old_string": "World", "new_string": "Rust"});
    let edit_result = edit_tool.execute(edit_args, &ctx).await.unwrap();
    assert!(edit_result["success"].as_bool().unwrap());

    // Verify
    let read_args = json!({"path": "edit.txt"});
    let read_result = read_tool.execute(read_args, &ctx).await.unwrap();
    assert_eq!(read_result["content"], "1| Hello Rust");
}

#[tokio::test]
async fn test_edit_file_not_found() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = EditFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({
        "path": "nonexistent.txt",
        "old_string": "old",
        "new_string": "new"
    });
    let result = tool.execute(args, &ctx).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_edit_file_old_string_not_found() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("file.txt"), "content here")
        .await
        .unwrap();

    let tool = EditFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({
        "path": "file.txt",
        "old_string": "未匹配的旧文本".repeat(12),
        "new_string": "replacement"
    });
    let result = tool.execute(args, &ctx).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
    assert_eq!(
        tokio::fs::read_to_string(mount_root.join("file.txt"))
            .await
            .unwrap(),
        "content here"
    );
}

#[tokio::test]
async fn test_edit_file_multiline_replacement() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("multi.txt"), "start\nmiddle\nend")
        .await
        .unwrap();

    let tool = EditFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({
        "path": "multi.txt",
        "old_string": "start\nmiddle",
        "new_string": "begin\ncenter"
    });
    let result = tool.execute(args, &ctx).await.unwrap();

    assert!(result["success"].as_bool().unwrap());

    let content = tokio::fs::read_to_string(mount_root.join("multi.txt"))
        .await
        .unwrap();
    assert_eq!(content, "begin\ncenter\nend");
}

#[tokio::test]
async fn test_edit_file_enforces_uniqueness_unless_replace_all_and_returns_diff() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(
        mount_root.join("repeat.txt"),
        "header\nfoo one\nmiddle\nfoo two\ntail\n",
    )
    .await
    .unwrap();

    let tool = EditFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let ambiguous = tool
        .execute(
            json!({
                "path": "repeat.txt",
                "old_string": "foo",
                "new_string": "bar"
            }),
            &ctx,
        )
        .await
        .unwrap_err();
    let msg = ambiguous.to_string();
    assert!(msg.contains("2 occurrences"), "{msg}");
    assert!(msg.contains("replace_all: true"), "{msg}");

    let all_result = tool
        .execute(
            json!({
                "path": "repeat.txt",
                "old_string": "foo",
                "new_string": "bar",
                "replace_all": true
            }),
            &ctx,
        )
        .await
        .unwrap();

    assert_eq!(all_result["replacements"], 2);
    let pres: alan_agent_protocol::ToolResultPresentation =
        serde_json::from_value(all_result["presentation"].clone()).unwrap();
    match pres {
        alan_agent_protocol::ToolResultPresentation::Diff { hunks, .. } => {
            assert_eq!(hunks.len(), 2);
            assert!(
                hunks[0]
                    .header
                    .as_deref()
                    .is_some_and(|h| h.starts_with("@@ -"))
            );
        }
        other => panic!("expected Diff presentation, got {other:?}"),
    }

    let content = tokio::fs::read_to_string(mount_root.join("repeat.txt"))
        .await
        .unwrap();
    assert_eq!(content, "header\nbar one\nmiddle\nbar two\ntail\n");
}

#[test]
fn test_is_image() {
    assert!(is_image(Path::new("test.png")));
    assert!(is_image(Path::new("test.jpg")));
    assert!(is_image(Path::new("test.JPEG")));
    assert!(is_image(Path::new("test.gif")));
    assert!(is_image(Path::new("test.webp")));
    assert!(is_image(Path::new("test.svg")));
    assert!(is_image(Path::new("test.bmp")));
    assert!(!is_image(Path::new("test.txt")));
    assert!(!is_image(Path::new("test")));
    assert!(!is_image(Path::new("")));
}

#[test]
fn test_detect_mime() {
    assert_eq!(detect_mime(Path::new("test.png")), "image/png");
    assert_eq!(detect_mime(Path::new("test.jpg")), "image/jpeg");
    assert_eq!(detect_mime(Path::new("test.jpeg")), "image/jpeg");
    assert_eq!(detect_mime(Path::new("test.gif")), "image/gif");
    assert_eq!(detect_mime(Path::new("test.webp")), "image/webp");
    assert_eq!(detect_mime(Path::new("test.svg")), "image/svg+xml");
    assert_eq!(detect_mime(Path::new("test.bmp")), "image/bmp");
    assert_eq!(
        detect_mime(Path::new("test.unknown")),
        "application/octet-stream"
    );
    assert_eq!(detect_mime(Path::new("test")), "application/octet-stream");
}

#[test]
fn test_read_file_tool_metadata() {
    let tool = ReadFileTool::new();
    assert_eq!(tool.name(), "read_file");
    assert_eq!(
        tool.capability(&json!({})),
        alan_agent_protocol::ToolCapability::Read
    );
}

#[test]
fn test_write_file_tool_metadata() {
    let tool = WriteFileTool::new();
    assert_eq!(tool.name(), "write_file");
    assert_eq!(
        tool.capability(&json!({})),
        alan_agent_protocol::ToolCapability::Write
    );
}

#[test]
fn test_edit_file_tool_metadata() {
    let tool = EditFileTool::new();
    assert_eq!(tool.name(), "edit_file");
    assert_eq!(
        tool.capability(&json!({})),
        alan_agent_protocol::ToolCapability::Write
    );
}
