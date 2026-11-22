use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use alan_agent_engine::Config;
use alan_agent_engine::tools::{
    TOOL_READ_ONLY_SYSTEM_PREFIXES, Tool, ToolContext, ToolExecutionBinding,
};
use alan_ap::{
    ErrorCode, Fid, FileKind, FileServer, InProcessTransport, Offset, OpenMode, Qid, Stat,
};
use alan_kernel::{Access, MountFs, Namespace};
use alan_tools::{EditFileTool, GlobTool, GrepTool, ListDirTool, ReadFileTool, WriteFileTool};
use async_trait::async_trait;
use serde_json::json;

/// FileServer exposing a blocking Stream and side-effecting Clone alongside a regular File.
struct HazardFs {
    stream_opens: Arc<AtomicUsize>,
    clone_opens: Arc<AtomicUsize>,
    fids: tokio::sync::Mutex<HashMap<Fid, HazardNode>>,
}

#[derive(Clone, Copy)]
enum HazardNode {
    Root,
    Status,
    Events,
    Clone,
}

impl HazardFs {
    fn new(stream_opens: Arc<AtomicUsize>, clone_opens: Arc<AtomicUsize>) -> Self {
        Self {
            stream_opens,
            clone_opens,
            fids: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    fn qid(node: HazardNode) -> Qid {
        match node {
            HazardNode::Root => Qid {
                kind: FileKind::Dir,
                version: 0,
                path: 1,
            },
            HazardNode::Status => Qid {
                kind: FileKind::File,
                version: 0,
                path: 2,
            },
            HazardNode::Events => Qid {
                kind: FileKind::Stream,
                version: 0,
                path: 3,
            },
            HazardNode::Clone => Qid {
                kind: FileKind::Clone,
                version: 0,
                path: 4,
            },
        }
    }
}

#[async_trait]
impl FileServer for HazardFs {
    async fn walk(&self, fid: Fid, newfid: Fid, names: &[String]) -> Result<Qid, ErrorCode> {
        let mut fids = self.fids.lock().await;
        let mut node = if fid == Fid::ROOT {
            HazardNode::Root
        } else {
            *fids.get(&fid).ok_or(ErrorCode::NotFound)?
        };
        for name in names {
            node = match (node, name.as_str()) {
                (HazardNode::Root, "status") => HazardNode::Status,
                (HazardNode::Root, "events") => HazardNode::Events,
                (HazardNode::Root, "clone") => HazardNode::Clone,
                (HazardNode::Root, _) => return Err(ErrorCode::NotFound),
                _ => return Err(ErrorCode::NotDirectory),
            };
        }
        fids.insert(newfid, node);
        Ok(Self::qid(node))
    }

    async fn open(&self, fid: Fid, _mode: OpenMode) -> Result<Qid, ErrorCode> {
        let node = {
            let fids = self.fids.lock().await;
            if fid == Fid::ROOT {
                HazardNode::Root
            } else {
                *fids.get(&fid).ok_or(ErrorCode::NotFound)?
            }
        };
        match node {
            HazardNode::Events => {
                self.stream_opens.fetch_add(1, Ordering::SeqCst);
                std::future::pending::<()>().await;
                unreachable!()
            }
            HazardNode::Clone => {
                self.clone_opens.fetch_add(1, Ordering::SeqCst);
                Ok(Self::qid(node))
            }
            _ => Ok(Self::qid(node)),
        }
    }

    async fn read(&self, fid: Fid, offset: Offset, count: u32) -> Result<Vec<u8>, ErrorCode> {
        let node = {
            let fids = self.fids.lock().await;
            if fid == Fid::ROOT {
                HazardNode::Root
            } else {
                *fids.get(&fid).ok_or(ErrorCode::NotFound)?
            }
        };
        let bytes: &[u8] = match node {
            HazardNode::Root => b"clone\nevents\nstatus",
            HazardNode::Status => b"running\n",
            HazardNode::Events => {
                std::future::pending::<()>().await;
                unreachable!()
            }
            HazardNode::Clone => b"allocated-id",
        };
        let start = (offset as usize).min(bytes.len());
        let end = bytes.len().min(start + count as usize);
        Ok(bytes[start..end].to_vec())
    }

    async fn write(&self, _fid: Fid, _offset: Offset, _data: &[u8]) -> Result<u32, ErrorCode> {
        Err(ErrorCode::NoAccess)
    }

    async fn stat(&self, fid: Fid) -> Result<Stat, ErrorCode> {
        let fids = self.fids.lock().await;
        let node = if fid == Fid::ROOT {
            HazardNode::Root
        } else {
            *fids.get(&fid).ok_or(ErrorCode::NotFound)?
        };
        let length = match node {
            HazardNode::Status => 8,
            _ => 0,
        };
        Ok(Stat {
            name: String::new(),
            qid: Self::qid(node),
            length,
            executable: false,
            writable: false,
        })
    }

    async fn create(
        &self,
        _fid: Fid,
        _newfid: Fid,
        _name: &str,
        _kind: FileKind,
    ) -> Result<Qid, ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn remove(&self, _fid: Fid) -> Result<(), ErrorCode> {
        Err(ErrorCode::Unsupported)
    }

    async fn clunk(&self, fid: Fid) -> Result<(), ErrorCode> {
        self.fids.lock().await.remove(&fid);
        Ok(())
    }
}

#[tokio::test]
async fn stream_and_clone_nodes_are_never_opened_or_blocked_on() {
    let stream_opens = Arc::new(AtomicUsize::new(0));
    let clone_opens = Arc::new(AtomicUsize::new(0));
    let hazard = Arc::new(HazardFs::new(stream_opens.clone(), clone_opens.clone()));

    let mut ns = Namespace::new();
    ns.mount(
        "/proc/8",
        InProcessTransport::new(hazard.clone()),
        Access::ReadWrite,
    );
    ns.mount(
        "/agent/8",
        InProcessTransport::new(hazard),
        Access::ReadWrite,
    );
    let view = Arc::new(MountFs::new(ns).with_read_only_prefixes(TOOL_READ_ONLY_SYSTEM_PREFIXES));
    let ctx = ToolContext::from_binding(
        ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), PathBuf::new()),
        Arc::new(Config::default()),
    )
    .with_namespace(8, InProcessTransport::new(view));

    tokio::time::timeout(Duration::from_secs(2), async {
        // list_dir inspects types via Stat only (no opens on stream/clone).
        let listed = ListDirTool::new()
            .execute(json!({"path": "/proc/8"}), &ctx)
            .await
            .unwrap();
        let types: Vec<(&str, &str)> = listed["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["name"].as_str().unwrap(), e["type"].as_str().unwrap()))
            .collect();
        assert_eq!(
            types,
            vec![("clone", "clone"), ("events", "stream"), ("status", "file")]
        );

        // grep across /proc and / finds regular files, skips /agent, and skips streams & clones.
        let grep_proc = GrepTool::new()
            .execute(json!({"pattern": "running", "path": "/proc"}), &ctx)
            .await
            .unwrap();
        assert_eq!(grep_proc["total"], 1);
        assert_eq!(grep_proc["matches"][0]["path"], "/proc/8/status");

        let grep_root = GrepTool::new()
            .execute(json!({"pattern": "running", "path": "/"}), &ctx)
            .await
            .unwrap();
        assert_eq!(grep_root["total"], 1);
        assert_eq!(grep_root["matches"][0]["path"], "/proc/8/status");

        // Direct grep on a stream or clone returns 0 matches without opening.
        assert_eq!(
            GrepTool::new()
                .execute(json!({"pattern": ".*", "path": "/proc/8/events"}), &ctx)
                .await
                .unwrap()["total"],
            0
        );
        assert_eq!(
            GrepTool::new()
                .execute(json!({"pattern": ".*", "path": "/proc/8/clone"}), &ctx)
                .await
                .unwrap()["total"],
            0
        );

        // glob skips streams & clones and returns only regular files.
        let glob_res = GlobTool::new()
            .execute(json!({"pattern": "**/*", "path": "/proc/8"}), &ctx)
            .await
            .unwrap();
        assert_eq!(glob_res["matches"], json!(["/proc/8/status"]));

        // read_file, write_file, and edit_file reject streams & clones without opening.
        for path in ["/proc/8/events", "/proc/8/clone"] {
            assert!(
                ReadFileTool::new()
                    .execute(json!({"path": path}), &ctx)
                    .await
                    .is_err()
            );
            assert!(
                WriteFileTool::new()
                    .execute(json!({"path": path, "content": "x"}), &ctx)
                    .await
                    .is_err()
            );
            assert!(
                EditFileTool::new()
                    .execute(
                        json!({"path": path, "old_string": "a", "new_string": "b"}),
                        &ctx
                    )
                    .await
                    .is_err()
            );
        }
    })
    .await
    .expect("operations must complete immediately without blocking on streams");

    assert_eq!(stream_opens.load(Ordering::SeqCst), 0);
    assert_eq!(clone_opens.load(Ordering::SeqCst), 0);
}
