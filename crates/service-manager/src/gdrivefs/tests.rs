use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use alan_ap::{ErrorCode, Fid, FileServer, InProcessTransport, OpenMode};
use alan_kernel::{Access, MountFs, Namespace};
use base64::Engine;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::backend::{
    DriveAuthSummary, DriveAuthToken, DriveFileMeta, DriveTokenFuture, DriveTokenProvider,
    GOOGLE_DRIVE_FILE_SCOPE, GOOGLE_DRIVE_READONLY_SCOPE, GdriveBackend, GdriveStatusSnapshot,
    MAX_EXPORT_BYTES, MemoryGdriveBackend, NewDriveUploadRequest, WORKSPACE_DOC_MIME,
    WORKSPACE_FOLDER_MIME, WORKSPACE_SCRIPT_MIME, WORKSPACE_SHEET_MIME, WORKSPACE_SLIDES_MIME,
    redact_google_tokens, valid_drive_id,
};
use super::{GdriveFs, GoogleDriveApiBackend};

fn sample_meta(id: &str, name: &str, mime_type: &str, parents: Vec<&str>) -> DriveFileMeta {
    DriveFileMeta {
        id: id.to_string(),
        name: name.to_string(),
        mime_type: mime_type.to_string(),
        is_folder: mime_type == WORKSPACE_FOLDER_MIME,
        is_workspace_doc: false,
        export_mime_type: None,
        size: None,
        parents: parents.into_iter().map(ToOwned::to_owned).collect(),
        modified_time: Some("2026-11-25T00:00:00Z".to_string()),
        md5_checksum: None,
        head_revision_id: Some("rev-1".to_string()),
        etag: Some(format!("\"etag-{id}-1\"")),
        web_view_link: Some(format!("https://drive.google.com/file/d/{id}/view")),
        trashed: false,
    }
}

static NEXT_TEST_FID: AtomicU64 = AtomicU64::new(100);

fn next_fid() -> Fid {
    Fid(NEXT_TEST_FID.fetch_add(1, Ordering::Relaxed))
}

async fn walk_path(fs: &dyn FileServer, path: &str) -> Result<Fid, ErrorCode> {
    let fid = next_fid();
    let names: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    fs.walk(Fid::ROOT, fid, &names).await?;
    Ok(fid)
}

async fn read_all(fs: &dyn FileServer, fid: Fid) -> Result<Vec<u8>, ErrorCode> {
    let mut out = Vec::new();
    let mut offset = 0u64;
    loop {
        let chunk = fs.read(fid, offset, 4096).await?;
        if chunk.is_empty() {
            break;
        }
        offset += chunk.len() as u64;
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

#[test]
fn id_upload_validation_and_token_redaction_fail_closed() {
    assert!(valid_drive_id("1aBcDeFg_9-0."));
    assert!(!valid_drive_id(""));
    assert!(!valid_drive_id("."));
    assert!(!valid_drive_id(".."));
    assert!(!valid_drive_id("a/b"));
    assert!(!valid_drive_id("bad id"));

    let bad_name = NewDriveUploadRequest {
        name: "../escape.txt".to_string(),
        parent_id: None,
        mime_type: "text/plain".to_string(),
        content: Some("hi".to_string()),
        content_base64: None,
    };
    assert_eq!(bad_name.validate_and_decode(), Err(ErrorCode::BadRequest));

    let both_bodies = NewDriveUploadRequest {
        name: "ok.txt".to_string(),
        parent_id: None,
        mime_type: "text/plain".to_string(),
        content: Some("hi".to_string()),
        content_base64: Some(base64::engine::general_purpose::STANDARD.encode(b"hi")),
    };
    assert_eq!(
        both_bodies.validate_and_decode(),
        Err(ErrorCode::BadRequest)
    );

    let raw = "err Bearer my.secret.jwt ya29.a0ARrdaM_secret 1//0g_refresh_secret access_token=tok123 explicit-key";
    let scrubbed = redact_google_tokens(raw, Some("explicit-key"));
    assert!(!scrubbed.contains("my.secret.jwt"), "{scrubbed}");
    assert!(!scrubbed.contains("ya29.a0ARrdaM_secret"), "{scrubbed}");
    assert!(!scrubbed.contains("1//0g_refresh_secret"), "{scrubbed}");
    assert!(!scrubbed.contains("tok123"), "{scrubbed}");
    assert!(!scrubbed.contains("explicit-key"), "{scrubbed}");

    let token = DriveAuthToken {
        access_token: "ya29.super-secret-token".to_string(),
        scopes: vec![GOOGLE_DRIVE_READONLY_SCOPE.to_string()],
        email: Some("user@example.com".to_string()),
    };
    assert!(!format!("{token:?}").contains("ya29.super-secret-token"));
}

#[tokio::test]
async fn readonly_mount_exports_workspace_docs_and_never_leaks_tokens() {
    let backend = Arc::new(MemoryGdriveBackend::new());
    backend.seed_file(
        sample_meta(
            "doc-1",
            "Architecture Notes",
            WORKSPACE_DOC_MIME,
            vec!["root"],
        ),
        b"# Architecture\n\n- Pure Rust\n",
    );
    backend.seed_file(
        sample_meta("sheet-1", "Budget", WORKSPACE_SHEET_MIME, vec!["root"]),
        b"item,cost\ncompute,42\n",
    );
    backend.seed_file(
        sample_meta("slides-1", "Deck", WORKSPACE_SLIDES_MIME, vec!["root"]),
        b"Slide 1: Alan OS\n",
    );
    backend.seed_file(
        sample_meta("script-1", "Macros", WORKSPACE_SCRIPT_MIME, vec!["root"]),
        br#"{"files":[{"name":"Code"}]}"#,
    );

    let gdrive = Arc::new(GdriveFs::new(backend));
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/gdrive",
        InProcessTransport::new(gdrive),
        Access::ReadOnly,
    );
    let mount = MountFs::new(ns);

    let doc_fid = walk_path(&mount, "/mnt/gdrive/by-id/doc-1/content")
        .await
        .unwrap();
    mount.open(doc_fid, OpenMode::Read).await.unwrap();
    let doc_bytes = read_all(&mount, doc_fid).await.unwrap();
    assert_eq!(doc_bytes, b"# Architecture\n\n- Pure Rust\n");
    mount.clunk(doc_fid).await.unwrap();

    let sheet_fid = walk_path(&mount, "/mnt/gdrive/files/Budget").await.unwrap();
    mount.open(sheet_fid, OpenMode::Read).await.unwrap();
    let sheet_bytes = read_all(&mount, sheet_fid).await.unwrap();
    assert_eq!(sheet_bytes, b"item,cost\ncompute,42\n");
    mount.clunk(sheet_fid).await.unwrap();

    let slides_fid = walk_path(&mount, "/mnt/gdrive/by-id/slides-1/content")
        .await
        .unwrap();
    mount.open(slides_fid, OpenMode::Read).await.unwrap();
    assert_eq!(
        read_all(&mount, slides_fid).await.unwrap(),
        b"Slide 1: Alan OS\n"
    );
    mount.clunk(slides_fid).await.unwrap();

    let script_fid = walk_path(&mount, "/mnt/gdrive/by-id/script-1/content")
        .await
        .unwrap();
    mount.open(script_fid, OpenMode::Read).await.unwrap();
    assert_eq!(
        read_all(&mount, script_fid).await.unwrap(),
        br#"{"files":[{"name":"Code"}]}"#
    );
    mount.clunk(script_fid).await.unwrap();

    let meta_fid = walk_path(&mount, "/mnt/gdrive/by-id/doc-1/meta.json")
        .await
        .unwrap();
    mount.open(meta_fid, OpenMode::Read).await.unwrap();
    let meta_json: DriveFileMeta =
        serde_json::from_slice(&read_all(&mount, meta_fid).await.unwrap()).unwrap();
    assert_eq!(meta_json.export_mime_type.as_deref(), Some("text/markdown"));
    mount.clunk(meta_fid).await.unwrap();

    let status_fid = walk_path(&mount, "/mnt/gdrive/status").await.unwrap();
    mount.open(status_fid, OpenMode::Read).await.unwrap();
    let status_raw = String::from_utf8(read_all(&mount, status_fid).await.unwrap()).unwrap();
    let status: GdriveStatusSnapshot = serde_json::from_str(&status_raw).unwrap();
    assert_eq!(status.auth.scopes, vec![GOOGLE_DRIVE_READONLY_SCOPE]);
    assert!(!status.auth.has_file_write_scope);
    assert!(!status_raw.contains("ya29."));
    assert!(!status_raw.contains("1//"));
    mount.clunk(status_fid).await.unwrap();

    // ReadOnly mount rejects upload writes and ctl mutations.
    let upload_fid = walk_path(&mount, "/mnt/gdrive/uploads/new").await.unwrap();
    assert_eq!(
        mount.open(upload_fid, OpenMode::Write).await.unwrap_err(),
        ErrorCode::NoAccess
    );
    mount.clunk(upload_fid).await.unwrap();
}

#[tokio::test]
async fn workspace_export_above_10mb_fails_explicitly() {
    let backend = Arc::new(MemoryGdriveBackend::new());
    backend.seed_file(
        sample_meta("huge-doc", "Huge Doc", WORKSPACE_DOC_MIME, vec!["root"]),
        vec![b'a'; MAX_EXPORT_BYTES + 1],
    );
    let gdrive = Arc::new(GdriveFs::new(backend));

    let fid = Fid(1);
    gdrive
        .walk(
            Fid::ROOT,
            fid,
            &["by-id".into(), "huge-doc".into(), "content".into()],
        )
        .await
        .unwrap();
    gdrive.open(fid, OpenMode::Read).await.unwrap();
    assert_eq!(
        gdrive.read(fid, 0, 4096).await.unwrap_err(),
        ErrorCode::BadRequest
    );
    assert_eq!(gdrive.stat(fid).await.unwrap_err(), ErrorCode::BadRequest);
    gdrive.clunk(fid).await.unwrap();

    let status_fid = Fid(2);
    gdrive
        .walk(Fid::ROOT, status_fid, &["status".into()])
        .await
        .unwrap();
    gdrive.open(status_fid, OpenMode::Read).await.unwrap();
    let status: GdriveStatusSnapshot =
        serde_json::from_slice(&read_all(gdrive.as_ref(), status_fid).await.unwrap()).unwrap();
    assert!(
        status
            .last_error
            .as_deref()
            .is_some_and(|e| e.contains("10 MB"))
    );
    gdrive.clunk(status_fid).await.unwrap();
}

#[tokio::test]
async fn binary_range_reads_verify_revision_and_etag() {
    let backend = Arc::new(MemoryGdriveBackend::new());
    let payload: Vec<u8> = (0u8..=255).cycle().take(10_000).collect();
    backend.seed_file(
        sample_meta(
            "bin-1",
            "archive.bin",
            "application/octet-stream",
            vec!["root"],
        ),
        payload.clone(),
    );
    let gdrive = Arc::new(GdriveFs::new(backend.clone()));

    let fid = Fid(10);
    gdrive
        .walk(
            Fid::ROOT,
            fid,
            &["by-id".into(), "bin-1".into(), "content".into()],
        )
        .await
        .unwrap();
    let st = gdrive.stat(fid).await.unwrap();
    assert_eq!(st.length, 10_000);
    assert_eq!(backend.recorded_range_reads().len(), 0);

    gdrive.open(fid, OpenMode::Read).await.unwrap();
    let part1 = gdrive.read(fid, 0, 1000).await.unwrap();
    let part2 = gdrive.read(fid, 1000, 1500).await.unwrap();
    assert_eq!(part1, payload[0..1000]);
    assert_eq!(part2, payload[1000..2500]);
    assert_eq!(
        backend.recorded_range_reads(),
        vec![("bin-1".into(), 0, 1000), ("bin-1".into(), 1000, 1500)]
    );

    // Mutate remote revision mid-read: subsequent range read must fail closed.
    backend.bump_file_revision("bin-1", "\"etag-bin-1-2\"", "rev-2");
    assert_eq!(
        gdrive.read(fid, 2500, 500).await.unwrap_err(),
        ErrorCode::Io
    );
    gdrive.clunk(fid).await.unwrap();
}

#[tokio::test]
async fn duplicate_filenames_disambiguate_deterministically_and_search_is_bounded() {
    let backend = Arc::new(MemoryGdriveBackend::new());
    backend.seed_file(
        sample_meta("folder-a", "Projects", WORKSPACE_FOLDER_MIME, vec!["root"]),
        b"",
    );
    backend.seed_file(
        sample_meta(
            "dup123_alpha",
            "Report",
            WORKSPACE_DOC_MIME,
            vec!["folder-a"],
        ),
        b"alpha report",
    );
    backend.seed_file(
        sample_meta(
            "dup123_beta",
            "Report",
            WORKSPACE_DOC_MIME,
            vec!["folder-a"],
        ),
        b"beta report",
    );
    backend.seed_file(
        sample_meta(
            "unique-1",
            "Quarterly Summary",
            WORKSPACE_DOC_MIME,
            vec!["folder-a"],
        ),
        b"quarterly report body",
    );

    let gdrive = Arc::new(GdriveFs::new(backend));
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/gdrive",
        InProcessTransport::new(gdrive),
        Access::ReadOnly,
    );
    let mount = MountFs::new(ns);

    let dir_fid = walk_path(&mount, "/mnt/gdrive/files/Projects")
        .await
        .unwrap();
    mount.open(dir_fid, OpenMode::Read).await.unwrap();
    let listing = String::from_utf8(read_all(&mount, dir_fid).await.unwrap()).unwrap();
    mount.clunk(dir_fid).await.unwrap();

    let lines: Vec<&str> = listing.lines().collect();
    assert_eq!(
        lines,
        vec!["Quarterly Summary", "Report__dup123_a", "Report__dup123_b",]
    );

    let alpha_fid = walk_path(&mount, "/mnt/gdrive/files/Projects/Report__dup123_a")
        .await
        .unwrap();
    mount.open(alpha_fid, OpenMode::Read).await.unwrap();
    assert_eq!(read_all(&mount, alpha_fid).await.unwrap(), b"alpha report");
    mount.clunk(alpha_fid).await.unwrap();

    let search_fid = walk_path(&mount, "/mnt/gdrive/search/quarterly%20report")
        .await
        .unwrap();
    mount.open(search_fid, OpenMode::Read).await.unwrap();
    let search_json: super::DriveSearchResponse =
        serde_json::from_slice(&read_all(&mount, search_fid).await.unwrap()).unwrap();
    assert_eq!(search_json.query, "quarterly report");
    assert_eq!(search_json.hits.len(), 1);
    assert_eq!(search_json.hits[0].id, "unique-1");
    mount.clunk(search_fid).await.unwrap();
}

#[tokio::test]
async fn uploads_require_readwrite_and_drive_file_scope_and_deletes_fail_closed() {
    let backend = Arc::new(MemoryGdriveBackend::new());
    backend.seed_file(
        sample_meta(
            "shared-dir",
            "Shared",
            WORKSPACE_FOLDER_MIME,
            vec!["other-parent"],
        ),
        b"",
    );
    backend.seed_file(
        sample_meta("shared-note", "note.txt", "text/plain", vec!["shared-dir"]),
        b"hello shared",
    );
    let gdrive = Arc::new(GdriveFs::new(backend.clone()));
    let mut ns = Namespace::new();
    ns.mount(
        "/mnt/gdrive",
        InProcessTransport::new(gdrive),
        Access::ReadWrite,
    );
    let mount = MountFs::new(ns);

    // 1. Even on ReadWrite mount, `uploads/new` fails closed without `drive.file` scope.
    let upload_fid = walk_path(&mount, "/mnt/gdrive/uploads/new").await.unwrap();
    assert_eq!(
        mount.open(upload_fid, OpenMode::Write).await.unwrap_err(),
        ErrorCode::NoAccess
    );
    mount.clunk(upload_fid).await.unwrap();
    assert_eq!(backend.file_count(), 2);

    // 2. Permanent delete / empty-trash / remove fail closed without modifying Drive state.
    for bad_cmd in [
        "delete shared-note\n",
        "rm shared-note\n",
        "trash shared-note\n",
        "empty-trash\n",
        "empty_trash\n",
    ] {
        let ctl_fid = walk_path(&mount, "/mnt/gdrive/ctl").await.unwrap();
        mount.open(ctl_fid, OpenMode::Write).await.unwrap();
        mount.write(ctl_fid, 0, bad_cmd.as_bytes()).await.unwrap();
        assert_eq!(
            mount.clunk(ctl_fid).await.unwrap_err(),
            ErrorCode::BadRequest
        );
    }
    let note_fid = walk_path(&mount, "/mnt/gdrive/by-id/shared-note/content")
        .await
        .unwrap();
    assert_eq!(
        mount.remove(note_fid).await.unwrap_err(),
        ErrorCode::Unsupported
    );
    assert_eq!(backend.file_count(), 2);

    // 3. Valid `ctl` commands (`pin`, `refresh`, `sync`) succeed.
    let ctl_fid = walk_path(&mount, "/mnt/gdrive/ctl").await.unwrap();
    mount.open(ctl_fid, OpenMode::Write).await.unwrap();
    mount.write(ctl_fid, 0, b"pin shared-dir\n").await.unwrap();
    mount.clunk(ctl_fid).await.unwrap();

    let pinned_fid = walk_path(&mount, "/mnt/gdrive/files/Shared/note.txt")
        .await
        .unwrap();
    mount.open(pinned_fid, OpenMode::Read).await.unwrap();
    assert_eq!(read_all(&mount, pinned_fid).await.unwrap(), b"hello shared");
    mount.clunk(pinned_fid).await.unwrap();

    // 4. Grant `drive.file` scope: commit-on-clunk upload succeeds.
    backend.set_scopes(vec![
        GOOGLE_DRIVE_READONLY_SCOPE.to_string(),
        GOOGLE_DRIVE_FILE_SCOPE.to_string(),
    ]);
    let upload_fid = walk_path(&mount, "/mnt/gdrive/uploads/new").await.unwrap();
    mount.open(upload_fid, OpenMode::Write).await.unwrap();
    let payload = serde_json::to_vec(&NewDriveUploadRequest {
        name: "uploaded-report.md".to_string(),
        parent_id: None,
        mime_type: "text/markdown".to_string(),
        content: Some("# Uploaded\n".to_string()),
        content_base64: None,
    })
    .unwrap();
    mount.write(upload_fid, 0, &payload).await.unwrap();
    mount.clunk(upload_fid).await.unwrap();
    assert_eq!(backend.file_count(), 3);

    let uploaded_fid = walk_path(&mount, "/mnt/gdrive/files/uploaded-report.md")
        .await
        .unwrap();
    mount.open(uploaded_fid, OpenMode::Read).await.unwrap();
    assert_eq!(
        read_all(&mount, uploaded_fid).await.unwrap(),
        b"# Uploaded\n"
    );
    mount.clunk(uploaded_fid).await.unwrap();
}

struct MockTokenProvider {
    token: std::sync::Mutex<String>,
    scopes: Vec<String>,
    refresh_calls: AtomicUsize,
}

impl DriveTokenProvider for MockTokenProvider {
    fn access_token(&self) -> DriveTokenFuture<'_> {
        Box::pin(async move {
            Ok(DriveAuthToken {
                access_token: self.token.lock().unwrap().clone(),
                scopes: self.scopes.clone(),
                email: Some("http-test@example.com".to_string()),
            })
        })
    }

    fn force_refresh(&self) -> DriveTokenFuture<'_> {
        Box::pin(async move {
            self.refresh_calls.fetch_add(1, Ordering::SeqCst);
            let mut guard = self.token.lock().unwrap();
            *guard = "ya29.refreshed-token".to_string();
            Ok(DriveAuthToken {
                access_token: guard.clone(),
                scopes: self.scopes.clone(),
                email: Some("http-test@example.com".to_string()),
            })
        })
    }

    fn auth_summary(&self) -> DriveAuthSummary {
        DriveAuthSummary {
            authenticated: true,
            scopes: self.scopes.clone(),
            has_file_write_scope: self.scopes.iter().any(|s| s == GOOGLE_DRIVE_FILE_SCOPE),
            email: Some("http-test@example.com".to_string()),
        }
    }
}

async fn read_http_req(
    socket: &mut tokio::net::TcpStream,
) -> (String, HashMap<String, String>, Vec<u8>) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let n = socket.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            header_end = pos + 4;
            break;
        }
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
    let mut lines = head.lines();
    let req_line = lines.next().unwrap_or("").to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }
    let content_len = headers
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < header_end + content_len {
        let n = socket.read(&mut chunk).await.unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf[header_end..].to_vec();
    (req_line, headers, body)
}

#[tokio::test]
async fn google_drive_api_backend_handles_refresh_range_export_and_redaction() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        let mut saw_first_401 = false;
        while let Ok((mut socket, _)) = listener.accept().await {
            let (req_line, headers, _body) = read_http_req(&mut socket).await;
            let auth = headers
                .get("authorization")
                .map(String::as_str)
                .unwrap_or("");
            if !saw_first_401 && auth == "Bearer ya29.initial-expired" {
                saw_first_401 = true;
                let resp = "HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\nConnection: close\r\n\r\nunauthorized";
                socket.write_all(resp.as_bytes()).await.unwrap();
                continue;
            }
            assert_eq!(auth, "Bearer ya29.refreshed-token");

            if req_line.starts_with("GET /drive/v3/files/doc-http/export") {
                assert!(req_line.contains("mimeType=text%2Fmarkdown"));
                let body = "# Exported Doc\n";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/markdown\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(resp.as_bytes()).await.unwrap();
            } else if req_line.starts_with("GET /drive/v3/files/oversized-http/export") {
                let body = "oversized";
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/csv\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    MAX_EXPORT_BYTES + 100,
                    body
                );
                socket.write_all(resp.as_bytes()).await.unwrap();
            } else if req_line.starts_with("GET /drive/v3/files/bin-http?alt=media") {
                assert_eq!(headers.get("range").map(String::as_str), Some("bytes=2-5"));
                assert_eq!(headers.get("if-match").map(String::as_str), Some("\"v1\""));
                let body = b"2345";
                let resp = format!(
                    "HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                socket.write_all(resp.as_bytes()).await.unwrap();
                socket.write_all(body).await.unwrap();
            } else if req_line.starts_with("GET /drive/v3/files/rate-http") {
                let body = "too many requests ya29.refreshed-token";
                let resp = format!(
                    "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 7\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(resp.as_bytes()).await.unwrap();
            } else if req_line.starts_with("GET /drive/v3/files/doc-http?") {
                let body = r#"{"id":"doc-http","name":"Doc","mimeType":"application/vnd.google-apps.document"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                socket.write_all(resp.as_bytes()).await.unwrap();
            } else {
                let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found";
                socket.write_all(resp.as_bytes()).await.unwrap();
            }
        }
    });

    let provider = Arc::new(MockTokenProvider {
        token: std::sync::Mutex::new("ya29.initial-expired".to_string()),
        scopes: vec![GOOGLE_DRIVE_READONLY_SCOPE.to_string()],
        refresh_calls: AtomicUsize::new(0),
    });
    let api = GoogleDriveApiBackend::with_base_urls(
        provider.clone(),
        format!("http://{addr}/drive/v3"),
        format!("http://{addr}/upload/drive/v3"),
    );

    let meta = api.get_file_meta("doc-http").await.unwrap();
    assert_eq!(provider.refresh_calls.load(Ordering::SeqCst), 1);
    assert_eq!(meta.export_mime_type.as_deref(), Some("text/markdown"));

    let exported = api
        .export_workspace_file(&meta, "text/markdown")
        .await
        .unwrap();
    assert_eq!(exported, b"# Exported Doc\n");

    let mut oversized_meta = sample_meta("oversized-http", "Big", WORKSPACE_SHEET_MIME, vec![]);
    oversized_meta.is_workspace_doc = true;
    oversized_meta.size = None;
    let export_err = api
        .export_workspace_file(&oversized_meta, "text/csv")
        .await
        .unwrap_err();
    assert_eq!(export_err.code, ErrorCode::BadRequest);

    let mut bin_meta = sample_meta("bin-http", "blob.bin", "application/octet-stream", vec![]);
    bin_meta.size = Some(10);
    bin_meta.etag = Some("\"v1\"".to_string());
    let range = api
        .read_binary_range(&bin_meta, 2, 4, Some("\"v1\""), None)
        .await
        .unwrap();
    assert_eq!(range.bytes, b"2345");

    let rate_err = api.get_file_meta("rate-http").await.unwrap_err();
    assert_eq!(rate_err.code, ErrorCode::RateLimited);
    assert_eq!(rate_err.retry_after_secs, Some(7));
    assert!(!rate_err.message.contains("ya29.refreshed-token"));
}
