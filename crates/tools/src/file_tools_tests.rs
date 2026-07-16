use super::*;
use crate::test_support::tool_context_with_root;
use alan_agent_engine::Config;
use alan_agent_engine::tools::{Tool, ToolContext};
use serde_json::json;
use std::path::Path;
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

    assert_eq!(result["path"], json!(mount_root.join("test.txt")));
    assert_eq!(result["content"], json!("bound"));
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
    let ctx = ToolContext::new(mount_root.clone(), mount_root.join("tmp"), config);

    let err = tool
        .execute(json!({"path": "test.txt"}), &ctx)
        .await
        .unwrap_err();

    assert!(
        err.to_string()
            .contains("Tool Process has no explicit sandbox grant")
    );
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

    assert_eq!(result["content"], "line2\nline3");
    assert_eq!(result["start_line"], 2);
    assert_eq!(result["end_line"], 3);
    assert_eq!(result["total_lines"], 5);
    assert!(result["truncated"].as_bool().unwrap());
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
    assert_eq!(read_result["content"], "Hello World");
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
    assert_eq!(read_result["content"], "Hello Rust");
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
        "old_string": "not present",
        "new_string": "replacement"
    });
    let result = tool.execute(args, &ctx).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
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
async fn test_edit_file_only_first_occurrence() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("repeat.txt"), "foo foo foo")
        .await
        .unwrap();

    let tool = EditFileTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({
        "path": "repeat.txt",
        "old_string": "foo",
        "new_string": "bar"
    });
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["replacements"], 1);

    let content = tokio::fs::read_to_string(mount_root.join("repeat.txt"))
        .await
        .unwrap();
    assert_eq!(content, "bar foo foo");
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
