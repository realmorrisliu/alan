use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alan_agent_engine::Config;
use alan_agent_engine::tools::alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, OpenMode, Qid, Stat,
};
use alan_agent_engine::tools::{Tool, ToolContext, ToolExecutionBinding};
use async_trait::async_trait;
use serde_json::json;

use super::{MAX_NAMESPACE_DIR_ENTRIES, NamespaceFs, parse_dir_names};
use crate::{GlobTool, GrepTool, ListDirTool};

#[derive(Clone)]
enum MockFid {
    Root,
    File(String),
}

/// Minimal read-only aP directory server exposing `entry_count` synthetic files
/// (`f00000` .. `f{entry_count-1:05}`), keeping the listing compact while
/// exceeding `MAX_NAMESPACE_DIR_ENTRIES`.
struct LargeDirFs {
    entry_count: usize,
    listing: Vec<u8>,
    fids: Mutex<HashMap<Fid, MockFid>>,
}

impl LargeDirFs {
    fn new(entry_count: usize) -> Self {
        let mut listing = String::with_capacity(entry_count * 7);
        for i in 0..entry_count {
            listing.push_str(&format!("f{i:05}\n"));
        }
        let mut fids = HashMap::new();
        fids.insert(Fid::ROOT, MockFid::Root);
        Self {
            entry_count,
            listing: listing.into_bytes(),
            fids: Mutex::new(fids),
        }
    }

    fn qid_for(entry: &MockFid) -> Qid {
        match entry {
            MockFid::Root => Qid {
                kind: FileKind::Dir,
                version: 1,
                path: 1,
            },
            MockFid::File(name) => {
                let idx = name[1..].parse::<u64>().unwrap_or(0);
                Qid {
                    kind: FileKind::File,
                    version: 1,
                    path: idx + 2,
                }
            }
        }
    }

    fn is_valid_file(&self, name: &str) -> bool {
        name.len() == 6
            && name.starts_with('f')
            && name[1..]
                .parse::<usize>()
                .is_ok_and(|idx| idx < self.entry_count)
    }
}

#[async_trait]
impl FileServer for LargeDirFs {
    async fn walk(
        &self,
        fid: Fid,
        newfid: Fid,
        names: &[String],
    ) -> std::result::Result<Qid, ErrorCode> {
        let mut fids = self.fids.lock().unwrap();
        let mut current = fids.get(&fid).cloned().ok_or(ErrorCode::NotFound)?;
        for name in names {
            current = match current {
                MockFid::Root if self.is_valid_file(name) => MockFid::File(name.clone()),
                _ => return Err(ErrorCode::NotFound),
            };
        }
        let qid = Self::qid_for(&current);
        fids.insert(newfid, current);
        Ok(qid)
    }

    async fn open(&self, fid: Fid, mode: OpenMode) -> std::result::Result<Qid, ErrorCode> {
        if mode != OpenMode::Read {
            return Err(ErrorCode::NoAccess);
        }
        let fids = self.fids.lock().unwrap();
        let entry = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        Ok(Self::qid_for(entry))
    }

    async fn read(
        &self,
        fid: Fid,
        offset: u64,
        count: u32,
    ) -> std::result::Result<Vec<u8>, ErrorCode> {
        let fids = self.fids.lock().unwrap();
        let entry = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        let bytes: &[u8] = match entry {
            MockFid::Root => &self.listing,
            MockFid::File(name) => name.as_bytes(),
        };
        let start = usize::try_from(offset).unwrap_or(bytes.len());
        if start >= bytes.len() {
            return Ok(Vec::new());
        }
        let end = start.saturating_add(count as usize).min(bytes.len());
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: u64, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let fids = self.fids.lock().unwrap();
        let entry = fids.get(&fid).ok_or(ErrorCode::NotFound)?;
        let (name, length) = match entry {
            MockFid::Root => ("/".to_string(), self.listing.len() as u64),
            MockFid::File(name) => (name.clone(), name.len() as u64),
        };
        Ok(Stat {
            qid: Self::qid_for(entry),
            name,
            length,
            writable: false,
            executable: false,
        })
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        if fid != Fid::ROOT {
            self.fids.lock().unwrap().remove(&fid);
        }
        Ok(())
    }
}

#[test]
fn parse_dir_names_distinguishes_exact_limit_and_truncation() {
    let (exact, truncated) = parse_dir_names("a\n.\n..\n\nb\na/b\nc\na\n", 3);
    assert_eq!(exact, vec!["a", "b", "c"]);
    assert!(!truncated);

    let (capped, truncated) = parse_dir_names("a\nb\nc\nd\n", 3);
    assert_eq!(capped, vec!["a", "b", "c"]);
    assert!(truncated);
}

#[tokio::test]
async fn directory_entry_truncation_propagates_through_helpers_and_tools() {
    // 1. Small limit verifies propagation even when walk max_entries exceeds dir cap.
    let small_transport = InProcessTransport::new(Arc::new(LargeDirFs::new(6)));
    let small_ns = NamespaceFs::new(small_transport);

    let (entries, list_truncated) = small_ns.list_dir_entries_limited("/", 4).await.unwrap();
    assert_eq!(entries.len(), 4);
    assert!(list_truncated);

    let (walk_files, walk_truncated) = small_ns
        .walk_files_with_dir_limit("/", false, false, &[], 4, 100, 4)
        .await
        .unwrap();
    assert_eq!(walk_files.len(), 4);
    assert!(walk_truncated);

    // Exact cap without overflow must NOT report truncated.
    let (exact_entries, exact_truncated) = small_ns.list_dir_entries_limited("/", 6).await.unwrap();
    assert_eq!(exact_entries.len(), 6);
    assert!(!exact_truncated);

    // 2. Production MAX_NAMESPACE_DIR_ENTRIES cap propagates to list_dir, grep, and glob.
    let overflow_count = MAX_NAMESPACE_DIR_ENTRIES + 5;
    let transport = InProcessTransport::new(Arc::new(LargeDirFs::new(overflow_count)));
    let ctx = ToolContext::from_binding(
        ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), PathBuf::new()),
        Arc::new(Config::default()),
    )
    .with_namespace(8, transport);

    let listed = ListDirTool::new()
        .execute(json!({"path": "/"}), &ctx)
        .await
        .unwrap();
    assert_eq!(listed["total"], MAX_NAMESPACE_DIR_ENTRIES);
    assert_eq!(listed["truncated"], true);

    let omitted_name = format!("f{:05}", overflow_count - 1);
    let grepped = GrepTool::new()
        .execute(
            json!({
                "pattern": omitted_name,
                "path": "/",
                "glob": omitted_name,
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(grepped["total"], 0);
    assert_eq!(grepped["truncated"], true);

    let globbed = GlobTool::new()
        .execute(
            json!({
                "pattern": omitted_name,
                "path": "/",
                "respect_gitignore": false
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(globbed["total"], 0);
    assert_eq!(globbed["truncated"], true);
}
