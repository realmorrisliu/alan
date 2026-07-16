use super::*;
use alan_ap::{Fid, InProcessTransport, OpenMode};
use alan_shell::Shell;

#[tokio::test]
async fn file_surface_commits_commands_on_clunk() {
    let service = PackageService::ephemeral("test").unwrap();
    let shell = Shell::new(InProcessTransport::new(service.file_server()));
    shell
        .write(
            "/ctl",
            &serde_json::to_vec(&PackageCommand::Install {
                request_id: "file-install".to_string(),
                package_id: "file-pack".to_string(),
                snapshot: native_snapshot("file-skill", "body"),
            })
            .unwrap(),
        )
        .await
        .unwrap();
    let catalog: PackageCatalog =
        serde_json::from_slice(&shell.cat("/catalog").await.unwrap()).unwrap();
    assert!(catalog.packages.contains_key("file-pack"));
}

#[tokio::test]
async fn file_surface_rejects_invalid_commands_on_clunk() {
    let service = PackageService::ephemeral("test").unwrap();
    let shell = Shell::new(InProcessTransport::new(service.file_server()));
    assert_eq!(
        shell
            .write(
                "/ctl",
                &serde_json::to_vec(&PackageCommand::Install {
                    request_id: "failed-install".to_string(),
                    package_id: "INVALID".to_string(),
                    snapshot: native_snapshot("failed", "body"),
                })
                .unwrap(),
            )
            .await,
        Err(ErrorCode::BadRequest)
    );
    let results: BTreeMap<String, PackageCommandResult> =
        serde_json::from_slice(&shell.cat("/result").await.unwrap()).unwrap();
    assert!(!results.contains_key("failed-install"));
}

#[tokio::test]
async fn file_surface_rejects_duplicate_request_documents_on_clunk() {
    let service = PackageService::ephemeral("test").unwrap();
    let shell = Shell::new(InProcessTransport::new(service.file_server()));
    let command = serde_json::to_vec(&PackageCommand::List {
        request_id: "duplicate-request".to_string(),
    })
    .unwrap();
    shell.write("/ctl", &command).await.unwrap();
    assert_eq!(
        shell.write("/ctl", &command).await,
        Err(ErrorCode::BadRequest)
    );
    assert_eq!(service.catalog().generation, 0);
}

#[tokio::test]
async fn file_surface_discards_buffer_after_oversized_write_fails() {
    let service = PackageService::ephemeral("test").unwrap();
    let fs = service.file_server();
    let fid = Fid(41);
    fs.walk(Fid::ROOT, fid, &["ctl".to_string()]).await.unwrap();
    fs.open(fid, OpenMode::Write).await.unwrap();
    let command = serde_json::to_vec(&PackageCommand::Install {
        request_id: "rejected-oversized-write".to_string(),
        package_id: "must-not-install".to_string(),
        snapshot: native_snapshot("must-not-install", "body"),
    })
    .unwrap();
    fs.write(fid, 0, &command).await.unwrap();
    assert_eq!(
        fs.write(fid, MAX_COMMAND_BYTES as u64, b"x").await,
        Err(ErrorCode::BadRequest)
    );

    fs.clunk(fid).await.unwrap();
    assert!(service.resolve("must-not-install").is_err());
}
