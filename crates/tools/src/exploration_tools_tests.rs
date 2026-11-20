use super::*;
use crate::test_support::tool_context_with_root;
use alan_agent_engine::Config;
use alan_agent_engine::tools::Tool;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_grep_tool() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create test file
    tokio::fs::write(
        mount_root.join("search.txt"),
        "hello world\nfoo bar\nhello rust",
    )
    .await
    .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "hello", "path": "search.txt"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 2);
}

#[tokio::test]
async fn test_grep_tool_case_insensitive() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("case.txt"), "Hello\nHELLO\nhello")
        .await
        .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "hello", "path": "case.txt", "case_sensitive": false});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 3);
}

#[tokio::test]
async fn test_grep_tool_case_sensitive() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("case.txt"), "Hello\nHELLO\nhello")
        .await
        .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "hello", "path": "case.txt", "case_sensitive": true});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 1);
    assert_eq!(result["matches"][0]["content"], "hello");
}

#[tokio::test]
async fn test_grep_tool_directory_recursive() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::create_dir(mount_root.join("src")).await.unwrap();
    tokio::fs::write(mount_root.join("src/a.rs"), "fn main() {}")
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("src/b.rs"), "fn helper() {}")
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("root.txt"), "fn root() {}")
        .await
        .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "fn ", "path": "."});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 3);
}

#[tokio::test]
async fn test_grep_tool_no_matches() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("file.txt"), "content here")
        .await
        .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "nomatch", "path": "file.txt"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 0);
    assert!(result["matches"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_grep_tool_invalid_regex() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "[invalid", "path": "."});
    let result = tool.execute(args, &ctx).await;

    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Invalid regex"));
}

#[tokio::test]
async fn test_grep_tool_skips_hidden_dirs() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::create_dir(mount_root.join(".hidden"))
        .await
        .unwrap();
    tokio::fs::write(mount_root.join(".hidden/secret.txt"), "secret content")
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("visible.txt"), "visible content")
        .await
        .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "content", "path": "."});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 1);
    assert!(
        result["matches"][0]["path"]
            .as_str()
            .unwrap()
            .contains("visible.txt")
    );
}

#[tokio::test]
async fn test_grep_tool_skips_binary_files() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create a binary file with some pattern in it
    let binary_content = vec![0x00, 0x01, 0x02, 0x03];
    tokio::fs::write(mount_root.join("data.bin"), binary_content)
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("text.txt"), "test data")
        .await
        .unwrap();

    let tool = GrepTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "data", "path": "."});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 1);
    assert!(
        result["matches"][0]["path"]
            .as_str()
            .unwrap()
            .contains("text.txt")
    );
}

#[tokio::test]
async fn test_glob_tool() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("a.rs"), "").await.unwrap();
    tokio::fs::write(mount_root.join("b.rs"), "").await.unwrap();
    tokio::fs::write(mount_root.join("c.txt"), "")
        .await
        .unwrap();

    let tool = GlobTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "*.rs"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 2);
}

#[tokio::test]
async fn test_glob_tool_recursive() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::create_dir(mount_root.join("src")).await.unwrap();
    tokio::fs::create_dir(mount_root.join("src/nested"))
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("src/a.rs"), "")
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("src/nested/b.rs"), "")
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("root.rs"), "")
        .await
        .unwrap();

    let tool = GlobTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "**/*.rs"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 3);
}

#[tokio::test]
async fn test_glob_tool_with_path() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::create_dir(mount_root.join("subdir"))
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("subdir/file.txt"), "")
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("root.txt"), "")
        .await
        .unwrap();

    let tool = GlobTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "*.txt", "path": "subdir"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 1);
    assert!(result["matches"][0].as_str().unwrap().contains("subdir"));
}

#[tokio::test]
async fn test_glob_tool_no_matches() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = GlobTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"pattern": "*.nonexistent"});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 0);
    assert!(result["matches"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_list_dir_tool() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create some files
    tokio::fs::write(mount_root.join("file1.txt"), "")
        .await
        .unwrap();
    tokio::fs::create_dir(mount_root.join("dir1"))
        .await
        .unwrap();

    let tool = ListDirTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "."});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 2);
}

#[tokio::test]
async fn test_list_dir_default_path() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    tokio::fs::write(mount_root.join("file.txt"), "")
        .await
        .unwrap();

    let tool = ListDirTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    // No path argument, should use cwd
    let args = json!({});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 1);
}

#[tokio::test]
async fn test_list_dir_empty() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = ListDirTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "."});
    let result = tool.execute(args, &ctx).await.unwrap();

    assert_eq!(result["total"], 0);
    assert!(result["entries"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_list_dir_sorting() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    // Create files and dirs in non-sorted order
    tokio::fs::write(mount_root.join("z.txt"), "")
        .await
        .unwrap();
    tokio::fs::create_dir(mount_root.join("a_dir"))
        .await
        .unwrap();
    tokio::fs::write(mount_root.join("m.txt"), "")
        .await
        .unwrap();
    tokio::fs::create_dir(mount_root.join("z_dir"))
        .await
        .unwrap();

    let tool = ListDirTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "."});
    let result = tool.execute(args, &ctx).await.unwrap();

    let entries = result["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 4);
    // Directories first, sorted alphabetically
    assert_eq!(entries[0]["name"], "a_dir");
    assert_eq!(entries[0]["type"], "directory");
    assert_eq!(entries[1]["name"], "z_dir");
    assert_eq!(entries[1]["type"], "directory");
    // Then files
    assert_eq!(entries[2]["name"], "m.txt");
    assert_eq!(entries[2]["type"], "file");
    assert_eq!(entries[3]["name"], "z.txt");
    assert_eq!(entries[3]["type"], "file");
}

#[tokio::test]
async fn test_list_dir_not_found() {
    let temp = TempDir::new().unwrap();
    let mount_root = temp.path().to_path_buf();

    let tool = ListDirTool::new();
    let config = Arc::new(Config::default());
    let ctx = tool_context_with_root(mount_root.clone(), mount_root.join("tmp"), config);

    let args = json!({"path": "nonexistent"});
    let result = tool.execute(args, &ctx).await;

    assert!(result.is_err());
}

#[test]
fn test_is_binary_file() {
    assert!(is_binary_file(Path::new("test.exe")));
    assert!(is_binary_file(Path::new("test.dll")));
    assert!(is_binary_file(Path::new("test.so")));
    assert!(is_binary_file(Path::new("test.dylib")));
    assert!(is_binary_file(Path::new("test.bin")));
    assert!(is_binary_file(Path::new("test.o")));
    assert!(is_binary_file(Path::new("test.a")));
    assert!(is_binary_file(Path::new("test.zip")));
    assert!(is_binary_file(Path::new("test.tar")));
    assert!(is_binary_file(Path::new("test.gz")));
    assert!(is_binary_file(Path::new("test.png")));
    assert!(is_binary_file(Path::new("test.pdf")));
    assert!(!is_binary_file(Path::new("test.txt")));
    assert!(!is_binary_file(Path::new("test.rs")));
    assert!(!is_binary_file(Path::new("test")));
}

#[test]
fn test_grep_tool_metadata() {
    let tool = GrepTool::new();
    assert_eq!(tool.name(), "grep");
    assert_eq!(
        tool.capability(&json!({})),
        alan_agent_protocol::ToolCapability::Read
    );
}

#[test]
fn test_glob_tool_metadata() {
    let tool = GlobTool::new();
    assert_eq!(tool.name(), "glob");
    assert_eq!(
        tool.capability(&json!({})),
        alan_agent_protocol::ToolCapability::Read
    );
}

#[test]
fn test_list_dir_tool_metadata() {
    let tool = ListDirTool::new();
    assert_eq!(tool.name(), "list_dir");
    assert_eq!(
        tool.capability(&json!({})),
        alan_agent_protocol::ToolCapability::Read
    );
}

#[tokio::test]
async fn test_grep_and_glob_respect_gitignore_filters_context_limit_and_mtime_sort() {
    let temp = TempDir::new().unwrap();
    let root = temp.path().to_path_buf();

    tokio::fs::write(root.join(".gitignore"), "*.log\n!keep.log\nignored_dir/\n")
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join("target/debug"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join("node_modules/pkg"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join("ignored_dir"))
        .await
        .unwrap();
    tokio::fs::create_dir_all(root.join("src")).await.unwrap();

    tokio::fs::write(root.join("target/debug/out.rs"), "needle in target\n")
        .await
        .unwrap();
    tokio::fs::write(
        root.join("node_modules/pkg/index.js"),
        "needle in node_modules\n",
    )
    .await
    .unwrap();
    tokio::fs::write(root.join("ignored_dir/skip.rs"), "needle in ignored_dir\n")
        .await
        .unwrap();
    tokio::fs::write(root.join("app.log"), "needle in ignored log\n")
        .await
        .unwrap();
    tokio::fs::write(root.join("keep.log"), "needle in kept log\n")
        .await
        .unwrap();
    tokio::fs::write(root.join("src/older.rs"), "before\nneedle one\nafter\n")
        .await
        .unwrap();
    tokio::fs::write(root.join("src/helper.ts"), "needle ts\n")
        .await
        .unwrap();

    // Ensure distinct modification times for mtime descending sort test.
    std::thread::sleep(std::time::Duration::from_millis(25));
    tokio::fs::write(root.join("src/newer.rs"), "needle two\n")
        .await
        .unwrap();

    let ctx = tool_context_with_root(root.clone(), root.join("tmp"), Arc::new(Config::default()));
    let grep = GrepTool::new();

    // Default gitignore + file_type rust + context lines
    let rs_hits = grep
        .execute(
            json!({
                "pattern": "needle",
                "path": ".",
                "file_type": "rust",
                "before_context": 1,
                "after_context": 1
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(rs_hits["total"], 2);
    assert!(!rs_hits["truncated"].as_bool().unwrap());
    let older_hit = rs_hits["matches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["path"].as_str().unwrap().ends_with("older.rs"))
        .unwrap();
    assert_eq!(older_hit["before"], json!(["before"]));
    assert_eq!(older_hit["after"], json!(["after"]));

    // glob filter + files_only + limit
    let files_only = grep
        .execute(
            json!({
                "pattern": "needle",
                "path": ".",
                "glob": "*.log",
                "files_only": true
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(files_only["total"], 1);
    assert_eq!(files_only["matches"], json!(["/mnt/source/keep.log"]));

    let limited = grep
        .execute(
            json!({
                "pattern": "needle",
                "path": ".",
                "limit": 1
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(limited["total"], 1);
    assert!(limited["truncated"].as_bool().unwrap());

    // Glob respects gitignore/target and sorts newest mtime first.
    let glob = GlobTool::new();
    let glob_res = glob
        .execute(json!({"pattern": "**/*.rs", "path": "."}), &ctx)
        .await
        .unwrap();
    assert_eq!(glob_res["total"], 2);
    assert_eq!(
        glob_res["matches"],
        json!(["/mnt/source/src/newer.rs", "/mnt/source/src/older.rs"])
    );
}
