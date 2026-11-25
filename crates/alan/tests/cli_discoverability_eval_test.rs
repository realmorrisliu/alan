use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;

use alan::cli::q::{QAction, QArgs, run_with_service};
use alan_agent_engine::runtime::tool_packages::{
    PackagedTool, TOOL_ARGUMENTS_ARGV, TOOL_RESULT_STDOUT_JSON, ToolExecutionHints,
    ToolPackageManifest,
};
use alan_agent_engine::tools::{Tool, ToolContext, ToolExecutionBinding};
use alan_os_host::SecretStore;
use alan_service_manager::{
    PackageCommand, PackageExportKind, PackageService, PackageSnapshot, PackageSnapshotEntry,
    RootBindingMode,
};
use tempfile::TempDir;

fn repo_packages_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../service-manager/packages")
}

fn isolated_alan_cmd(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_alan"));
    cmd.env("HOME", root.join("home"))
        .env("XDG_DATA_HOME", root.join("xdg_data"))
        .env("TMPDIR", root.join("tmp"))
        .env_remove("ALAN_INSTALL_CHANNEL")
        .env_remove("ALAN_CONFIG_PATH")
        .env_remove("BASH_ENV");
    std::fs::create_dir_all(root.join("home")).unwrap();
    std::fs::create_dir_all(root.join("xdg_data")).unwrap();
    std::fs::create_dir_all(root.join("tmp")).unwrap();
    cmd
}

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
fn run_wrapper_with_retry(wrapper: &Path, args: &[&str], path_env: &str) -> Output {
    const TEXT_FILE_BUSY: i32 = 26;
    for attempt in 0..5 {
        match Command::new(wrapper)
            .args(args)
            .env("PATH", path_env)
            .output()
        {
            Ok(out) => return out,
            Err(err) if err.raw_os_error() == Some(TEXT_FILE_BUSY) && attempt < 4 => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(err) => panic!("failed running {}: {err}", wrapper.display()),
        }
    }
    unreachable!("retry loop returns or panics")
}

fn packaged_tool_from_repo(
    package_id: &str,
    allow_network: bool,
    secret_store: Option<Arc<SecretStore>>,
) -> (TempDir, PackagedTool) {
    let service = PackageService::ephemeral().unwrap();
    let pkg_dir = repo_packages_dir().join(package_id);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(run_with_service(
        &service,
        QArgs {
            action: QAction::Install {
                name: None,
                catalog_only: false,
                path: pkg_dir.clone(),
            },
        },
    ))
    .unwrap();
    let record = service.resolve(package_id).unwrap();
    let export = record.tool_exports.first().unwrap();
    let manifest = ToolPackageManifest {
        version: ToolPackageManifest::schema_version(),
        name: export.name.clone(),
        description: export.description.clone(),
        parameters: export.parameters.clone(),
        capability: export.capability,
        capability_is_argument_dependent: export.capability_is_argument_dependent,
        timeout_secs: export.timeout_secs,
        execution: ToolExecutionHints {
            arguments: export.arguments.clone(),
            result: TOOL_RESULT_STDOUT_JSON.to_string(),
        },
        network_domains: export.network_domains.clone(),
        required_secrets: export.required_secrets.clone(),
        required_adc: export.required_adc.clone(),
    };

    let tmp = tempfile::tempdir().unwrap();
    let source_wrapper = pkg_dir.join("bin").join(package_id);
    let installed_wrapper = tmp.path().join(package_id);
    std::fs::copy(&source_wrapper, &installed_wrapper).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&installed_wrapper, std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    let mut tool = PackagedTool::new(manifest, installed_wrapper, allow_network).unwrap();
    if let Some(store) = secret_store {
        tool = tool.with_secret_resolver(store);
    }
    (tmp, tool)
}

#[tokio::test]
async fn reference_packages_validate_and_default_to_skills_only_zero_authority() {
    let packages_dir = repo_packages_dir();

    // 1. gemini-api
    {
        let service = PackageService::ephemeral().unwrap();
        let out = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: None,
                    catalog_only: false,
                    path: packages_dir.join("gemini-api"),
                },
            },
        )
        .await
        .unwrap();
        assert!(out.contains("root_skills: attached (gemini-api)"), "{out}");
        assert!(
            out.contains("alan q enable gemini-api --allow-network"),
            "{out}"
        );

        let record = service.resolve("gemini-api").unwrap();
        assert_eq!(record.export_kind, PackageExportKind::Tool);
        assert_eq!(record.tool_exports.len(), 1);
        let export = &record.tool_exports[0];
        assert_eq!(export.name, "gemini_api");
        assert_eq!(export.arguments, TOOL_ARGUMENTS_ARGV);
        assert!(export.allow_network);
        assert_eq!(
            export.network_domains,
            vec!["generativelanguage.googleapis.com".to_string()]
        );
        assert_eq!(export.required_secrets.len(), 1);
        assert_eq!(export.required_secrets[0].env, "GEMINI_API_KEY");
        assert_eq!(export.required_secrets[0].credential_id, "gemini-api-key");
        assert_eq!(
            service.root_binding("gemini-api").unwrap().unwrap().mode,
            RootBindingMode::SkillsOnly
        );

        // Network-enabled tool packages require `--allow-network` and reject `--access`.
        let no_net = run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "gemini-api".to_string(),
                    access: None,
                    allow_network: false,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(no_net.contains("--allow-network"), "{no_net}");

        let bad_access = run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "gemini-api".to_string(),
                    access: Some(alan::cli::q::FileserverAccessArg::ReadOnly),
                    allow_network: true,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap_err()
        .to_string();
        assert!(bad_access.contains("--access"), "{bad_access}");

        let enabled = run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "gemini-api".to_string(),
                    access: None,
                    allow_network: true,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap();
        assert!(enabled.contains("allow_network=true"), "{enabled}");
        assert_eq!(
            service.root_binding("gemini-api").unwrap().unwrap().mode,
            RootBindingMode::Active
        );
    }

    // 2. colab-compute
    {
        let service = PackageService::ephemeral().unwrap();
        let out = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: None,
                    catalog_only: false,
                    path: packages_dir.join("colab-compute"),
                },
            },
        )
        .await
        .unwrap();
        assert!(
            out.contains("root_skills: attached (colab-compute)"),
            "{out}"
        );
        assert!(
            out.contains("alan q enable colab-compute --allow-network"),
            "{out}"
        );

        let record = service.resolve("colab-compute").unwrap();
        assert_eq!(record.export_kind, PackageExportKind::Tool);
        assert_eq!(record.tool_exports.len(), 1);
        let export = &record.tool_exports[0];
        assert_eq!(export.name, "colab_compute");
        assert_eq!(export.arguments, TOOL_ARGUMENTS_ARGV);
        assert!(export.allow_network);
        assert_eq!(
            export.network_domains,
            vec![
                "colab.pa.googleapis.com".to_string(),
                "colab.research.google.com".to_string(),
                "colaboratory.googleapis.com".to_string(),
                "oauth2.googleapis.com".to_string(),
                "www.googleapis.com".to_string(),
            ]
        );
        let adc = export.required_adc.as_ref().unwrap();
        assert_eq!(adc.env, "GOOGLE_APPLICATION_CREDENTIALS");
        assert_eq!(adc.credential_id, "colab-adc");
        assert_eq!(
            service.root_binding("colab-compute").unwrap().unwrap().mode,
            RootBindingMode::SkillsOnly
        );

        assert!(
            run_with_service(
                &service,
                QArgs {
                    action: QAction::Enable {
                        package_id: "colab-compute".to_string(),
                        access: None,
                        allow_network: false,
                        skills_only: false,
                    },
                },
            )
            .await
            .is_err()
        );
        let enabled = run_with_service(
            &service,
            QArgs {
                action: QAction::Enable {
                    package_id: "colab-compute".to_string(),
                    access: None,
                    allow_network: true,
                    skills_only: false,
                },
            },
        )
        .await
        .unwrap();
        assert!(enabled.contains("allow_network=true"), "{enabled}");
        assert_eq!(
            service.root_binding("colab-compute").unwrap().unwrap().mode,
            RootBindingMode::Active
        );
    }

    // 3. alan-arena
    {
        let service = PackageService::ephemeral().unwrap();
        let out = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: None,
                    catalog_only: false,
                    path: packages_dir.join("alan-arena"),
                },
            },
        )
        .await
        .unwrap();
        assert!(out.contains("root_skills: attached (alan-arena)"), "{out}");
        assert!(!out.contains("runtime: inactive"), "{out}");

        let record = service.resolve("alan-arena").unwrap();
        assert_eq!(record.export_kind, PackageExportKind::Skill);
        assert!(record.tool_exports.is_empty());
        assert!(record.fileserver_export.is_none());
        assert_eq!(
            service.root_binding("alan-arena").unwrap().unwrap().mode,
            RootBindingMode::SkillsOnly
        );
    }

    // 4. gdrive-fileserver
    {
        let service = PackageService::ephemeral().unwrap();
        let out = run_with_service(
            &service,
            QArgs {
                action: QAction::Install {
                    name: None,
                    catalog_only: false,
                    path: packages_dir.join("gdrive-fileserver"),
                },
            },
        )
        .await
        .unwrap();
        assert!(out.contains("root_skills: attached"), "{out}");
        assert!(out.contains("runtime: inactive"), "{out}");

        let record = service.resolve("gdrive-fileserver").unwrap();
        assert_eq!(record.export_kind, PackageExportKind::Fileserver);
        let fs_export = record.fileserver_export.as_ref().unwrap();
        assert_eq!(fs_export.service_id, "gdrive");
        assert!(fs_export.allow_network);
        assert_eq!(
            fs_export.network_domains,
            vec![
                "oauth2.googleapis.com".to_string(),
                "www.googleapis.com".to_string()
            ]
        );
        assert_eq!(
            service
                .root_binding("gdrive-fileserver")
                .unwrap()
                .unwrap()
                .mode,
            RootBindingMode::SkillsOnly
        );

        let inspected = run_with_service(
            &service,
            QArgs {
                action: QAction::Inspect {
                    package_id: "gdrive-fileserver".to_string(),
                },
            },
        )
        .await
        .unwrap();
        assert!(
            inspected.contains("root_binding: skills_only"),
            "{inspected}"
        );
        assert!(inspected.contains("mount_path: /mnt/gdrive"), "{inspected}");
    }

    // 5. Negative fail-closed manifest validation (empty network_domains and raw token secret).
    let service = PackageService::ephemeral().unwrap();
    let empty_domains = PackageSnapshot {
        source_name: "bad-net-tool".to_string(),
        entries: vec![
            PackageSnapshotEntry {
                path: "alan-package.toml".to_string(),
                bytes: br#"
kind = "tool"
[tool]
name = "bad_net"
description = "Missing domains"
entrypoint = "bin/run"
capability = "network"
allow_network = true
arguments = "argv"
"#
                .to_vec(),
                executable: false,
            },
            PackageSnapshotEntry {
                path: "bin/run".to_string(),
                bytes: b"#!/bin/sh\necho '{}'\n".to_vec(),
                executable: true,
            },
        ],
    };
    let net_res = service
        .execute(PackageCommand::Install {
            request_id: "bad-net".to_string(),
            package_id: "bad-net-tool".to_string(),
            snapshot: empty_domains,
        })
        .unwrap();
    assert!(!net_res.success);

    let raw_secret = PackageSnapshot {
        source_name: "bad-secret-tool".to_string(),
        entries: vec![
            PackageSnapshotEntry {
                path: "alan-package.toml".to_string(),
                bytes: br#"
kind = "tool"
[tool]
name = "bad_secret"
description = "Raw token"
entrypoint = "bin/run"
arguments = "argv"

[[tool.required_secrets]]
env = "GEMINI_API_KEY"
credential_id = "AIzaSyFakeInlineSecretToken"
"#
                .to_vec(),
                executable: false,
            },
            PackageSnapshotEntry {
                path: "bin/run".to_string(),
                bytes: b"#!/bin/sh\necho '{}'\n".to_vec(),
                executable: true,
            },
        ],
    };
    let secret_res = service
        .execute(PackageCommand::Install {
            request_id: "bad-secret".to_string(),
            package_id: "bad-secret-tool".to_string(),
            snapshot: raw_secret,
        })
        .unwrap();
    assert!(!secret_res.success);
}

#[cfg(unix)]
#[test]
fn gemini_api_wrapper_enforces_no_interactive_dry_run_and_diagnostics() {
    let wrapper = repo_packages_dir().join("gemini-api/bin/gemini-api");

    // 1. Missing host `gemini-api` binary emits actionable install & secret hint.
    let empty_bin = tempfile::tempdir().unwrap();
    let missing = run_wrapper_with_retry(&wrapper, &["whoami"], empty_bin.path().to_str().unwrap());
    assert_eq!(missing.status.code(), Some(127));
    let missing_err = String::from_utf8_lossy(&missing.stderr);
    assert!(
        missing_err.contains("gemini-api")
            && missing_err.contains("alan q set-secret gemini-api-key"),
        "expected actionable install hint, got: {missing_err}"
    );

    // 2. Rejects `--interactive` fail-closed.
    let fake_bin = tempfile::tempdir().unwrap();
    write_executable(
        &fake_bin.path().join("gemini-api"),
        "#!/usr/bin/env python3\nimport json, sys\nprint(json.dumps({'argv': sys.argv[1:]}))\n",
    );
    let fake_path = format!("{}:/usr/bin:/bin", fake_bin.path().display());
    let interactive =
        run_wrapper_with_retry(&wrapper, &["--interactive", "generate", "hi"], &fake_path);
    assert_eq!(interactive.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&interactive.stderr).contains("--no-interactive"),
        "{}",
        String::from_utf8_lossy(&interactive.stderr)
    );

    // 3. Confined `PackagedTool` execution: missing key fails deterministically without leaking.
    let cred_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SecretStore::from_directory(cred_dir.path()).unwrap());
    let (_pkg_tmp, missing_key_tool) =
        packaged_tool_from_repo("gemini-api", true, Some(store.clone()));
    let scratch = tempfile::tempdir().unwrap();
    let binding =
        ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), scratch.path().into());
    let ctx = ToolContext::from_binding(binding, Arc::new(alan_agent_engine::Config::default()));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let missing_key_err = rt
        .block_on(missing_key_tool.execute(
            serde_json::json!({"args": ["generate", "--dry-run", "-o", "json", "hi"]}),
            &ctx,
        ))
        .unwrap_err()
        .to_string();
    assert!(
        missing_key_err.contains("alan q set-secret gemini-api-key"),
        "{missing_key_err}"
    );

    // 4. With Host Store secret set, `PackagedTool` injects GEMINI_API_KEY and wrapper enforces `--no-interactive`.
    store
        .set_named_secret("gemini-api-key", "gm-test-secret-xyz")
        .unwrap();
    let (_pkg_tmp2, tool) = packaged_tool_from_repo("gemini-api", true, Some(store));
    let wrapper_check = run_wrapper_with_retry(
        &wrapper,
        &["generate", "--dry-run", "-o", "json", "hello"],
        &fake_path,
    );
    assert!(wrapper_check.status.success());
    let wrapper_json: serde_json::Value = serde_json::from_slice(&wrapper_check.stdout).unwrap();
    assert_eq!(
        wrapper_json["argv"],
        serde_json::json!([
            "--no-interactive",
            "generate",
            "--dry-run",
            "-o",
            "json",
            "hello"
        ])
    );

    // Also verify secret injection + stderr secret scrubbing via PackagedTool on an offline echo script.
    let script_dir = tempfile::tempdir().unwrap();
    let probe_script = script_dir.path().join("probe.py");
    write_executable(
        &probe_script,
        r#"#!/usr/bin/env python3
import json, os, sys
if "--fail" in sys.argv:
    sys.stderr.write("leak:" + os.environ.get("GEMINI_API_KEY", ""))
    sys.exit(3)
print(json.dumps({
    "dry_run": True,
    "has_key": os.environ.get("GEMINI_API_KEY") == "gm-test-secret-xyz",
    "has_proxy": "HTTP_PROXY" in os.environ,
    "argv": sys.argv[1:]
}))
"#,
    );
    let mut probe_manifest = tool.manifest().clone();
    probe_manifest.name = "gemini_probe".to_string();
    let probe_store = Arc::new(SecretStore::from_directory(cred_dir.path()).unwrap());
    let probe_tool = PackagedTool::new(probe_manifest, probe_script, true)
        .unwrap()
        .with_secret_resolver(probe_store);
    let ok = rt
        .block_on(probe_tool.execute(
            serde_json::json!({"args": ["--no-interactive", "generate", "--dry-run", "hi"]}),
            &ctx,
        ))
        .unwrap();
    assert_eq!(ok["dry_run"], serde_json::json!(true));
    assert_eq!(ok["has_key"], serde_json::json!(true));
    assert_eq!(ok["has_proxy"], serde_json::json!(true));

    let err = rt
        .block_on(probe_tool.execute(serde_json::json!({"args": ["--fail"]}), &ctx))
        .unwrap_err()
        .to_string();
    assert!(!err.contains("gm-test-secret-xyz"), "secret leaked: {err}");
    assert!(err.contains("[REDACTED]"), "{err}");
}

#[cfg(unix)]
#[test]
fn colab_compute_wrapper_enforces_auth_adc_and_ephemeral_adc_projection() {
    let wrapper = repo_packages_dir().join("colab-compute/bin/colab-compute");

    // 1. Missing host `colab` binary emits actionable install & ADC hint.
    let empty_bin = tempfile::tempdir().unwrap();
    let missing = run_wrapper_with_retry(&wrapper, &["usage"], empty_bin.path().to_str().unwrap());
    assert_eq!(missing.status.code(), Some(127));
    let missing_err = String::from_utf8_lossy(&missing.stderr);
    assert!(
        missing_err.contains("google-colab-cli")
            && missing_err.contains("alan q set-secret colab-adc"),
        "{missing_err}"
    );

    // 2. Rejects overriding `--auth` away from `adc`.
    let fake_bin = tempfile::tempdir().unwrap();
    write_executable(
        &fake_bin.path().join("colab"),
        "#!/usr/bin/env python3\nimport json, sys\nprint(json.dumps({'argv': sys.argv[1:]}))\n",
    );
    let fake_path = format!("{}:/usr/bin:/bin", fake_bin.path().display());
    for bad_args in [
        vec!["--auth=keyring", "usage"],
        vec!["--auth", "oauth", "usage"],
    ] {
        let bad = run_wrapper_with_retry(&wrapper, &bad_args, &fake_path);
        assert_eq!(bad.status.code(), Some(2));
        assert!(
            String::from_utf8_lossy(&bad.stderr).contains("--auth=adc"),
            "{}",
            String::from_utf8_lossy(&bad.stderr)
        );
    }

    // 3. Wrapper prepends `--auth=adc` cleanly.
    let ok_wrap = run_wrapper_with_retry(&wrapper, &["usage"], &fake_path);
    assert!(ok_wrap.status.success());
    let wrap_json: serde_json::Value = serde_json::from_slice(&ok_wrap.stdout).unwrap();
    assert_eq!(
        wrap_json["argv"],
        serde_json::json!(["--auth=adc", "usage"])
    );

    // 4. `PackagedTool` fails deterministically when `colab-adc` is absent, and projects 0600 file when set.
    let cred_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(SecretStore::from_directory(cred_dir.path()).unwrap());
    let (_pkg_tmp, missing_adc_tool) =
        packaged_tool_from_repo("colab-compute", true, Some(store.clone()));
    let scratch = tempfile::tempdir().unwrap();
    let binding =
        ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), scratch.path().into());
    let ctx = ToolContext::from_binding(binding, Arc::new(alan_agent_engine::Config::default()));
    let rt = tokio::runtime::Runtime::new().unwrap();
    let missing_adc_err = rt
        .block_on(missing_adc_tool.execute(serde_json::json!({"args": ["usage"]}), &ctx))
        .unwrap_err()
        .to_string();
    assert!(
        missing_adc_err.contains("alan q set-secret colab-adc"),
        "{missing_adc_err}"
    );

    store
        .set_named_secret(
            "colab-adc",
            r#"{"type":"authorized_user","client_id":"cid","client_secret":"cs-colab-secret-99","refresh_token":"rt-colab-token-99"}"#,
        )
        .unwrap();
    let script_dir = tempfile::tempdir().unwrap();
    let adc_probe = script_dir.path().join("adc_probe.py");
    write_executable(
        &adc_probe,
        r#"#!/usr/bin/env python3
import json, os, stat, sys
adc_path = os.environ["GOOGLE_APPLICATION_CREDENTIALS"]
mode = stat.S_IMODE(os.stat(adc_path).st_mode)
with open(adc_path) as f:
    payload = json.load(f)
print(json.dumps({
    "adc_path": adc_path,
    "mode": oct(mode),
    "type": payload["type"],
    "argv": sys.argv[1:],
}))
"#,
    );
    let mut probe_manifest = missing_adc_tool.manifest().clone();
    probe_manifest.name = "colab_probe".to_string();
    let adc_tool = PackagedTool::new(probe_manifest, adc_probe, true)
        .unwrap()
        .with_secret_resolver(store);
    let adc_out = rt
        .block_on(adc_tool.execute(serde_json::json!({"args": ["--auth=adc", "usage"]}), &ctx))
        .unwrap();
    assert_eq!(adc_out["mode"], serde_json::json!("0o600"));
    assert_eq!(adc_out["type"], serde_json::json!("authorized_user"));
    let projected_path = PathBuf::from(adc_out["adc_path"].as_str().unwrap());
    assert!(
        !projected_path.exists(),
        "ephemeral ADC file must be unlinked after execution: {}",
        projected_path.display()
    );
}

#[test]
fn cli_q_set_secret_reads_stdin_without_echoing_and_persists_in_host_store() {
    let root = tempfile::tempdir().unwrap();

    // 1. Single-line API key via piped stdin.
    let api_secret = "gm-super-secret-value-12345";
    let mut child = isolated_alan_cmd(root.path())
        .args(["q", "set-secret", "gemini-api-key"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(format!("{api_secret}\n").as_bytes())
            .unwrap();
    }
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stdout.contains(api_secret), "secret echoed to stdout");
    assert!(!stderr.contains(api_secret), "secret echoed to stderr");
    assert!(stdout.contains("gemini-api-key"), "{stdout}");

    // Verify Host Store location (`XDG_DATA_HOME/Alan/Host Store/credentials` or `HOME/Library/...`).
    let credentials_dir = if cfg!(target_os = "macos") {
        root.path()
            .join("home/Library/Application Support/Alan/Host Store/credentials")
    } else {
        root.path().join("xdg_data/Alan/Host Store/credentials")
    };
    let store = SecretStore::from_directory(&credentials_dir).unwrap();
    assert_eq!(
        store
            .resolve_named_secret("gemini-api-key")
            .unwrap()
            .as_deref(),
        Some(api_secret)
    );

    // 2. Multi-line ADC JSON via piped stdin.
    let adc_json = "{\n  \"type\": \"authorized_user\",\n  \"client_id\": \"cid\",\n  \"client_secret\": \"cs-secret-888\",\n  \"refresh_token\": \"rt-token-888\"\n}\n";
    let mut adc_child = isolated_alan_cmd(root.path())
        .args(["q", "set-secret", "colab-adc"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        adc_child
            .stdin
            .as_mut()
            .unwrap()
            .write_all(adc_json.as_bytes())
            .unwrap();
    }
    let adc_out = adc_child.wait_with_output().unwrap();
    assert!(
        adc_out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&adc_out.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&adc_out.stdout).contains("cs-secret-888"),
        "ADC secret echoed to stdout"
    );
    let adc_tmp = tempfile::tempdir().unwrap();
    let adc_file = store
        .project_ephemeral_adc_file("colab-adc", adc_tmp.path())
        .unwrap()
        .expect("colab-adc should project cleanly");
    assert!(adc_file.path().is_file());
    drop(adc_file);

    // 3. Empty stdin and unsafe credential_id fail cleanly.
    let mut empty_child = isolated_alan_cmd(root.path())
        .args(["q", "set-secret", "gemini-api-key"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(empty_child.stdin.take());
    let empty_out = empty_child.wait_with_output().unwrap();
    assert!(!empty_out.status.success());

    let bad_id = isolated_alan_cmd(root.path())
        .args(["q", "set-secret", "../escape"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!bad_id.status.success());
}

struct CliHelpEvalCase {
    task: &'static str,
    help_args: &'static [&'static [&'static str]],
    required_tokens: &'static [&'static str],
    max_help_bytes: usize,
}

#[test]
fn cli_help_discoverability_turn_budget_eval_fixture() {
    // Inspired by `gemini-api-cli` (`evals/harness-comparison`): verify that an agent
    // or operator can discover `alan q` and non-interactive `alan -p` invocation
    // surfaces within a strict <= 2 `--help` lookup turn budget and bounded output size.
    let root = tempfile::tempdir().unwrap();
    let cases = [
        CliHelpEvalCase {
            task: "Discover Quartermaster hub and non-interactive print flags from top-level help",
            help_args: &[&["--help"]],
            required_tokens: &[
                "--print",
                "-p",
                "--json",
                "--ephemeral",
                "--package",
                "--fileserver",
                "--image",
                "q",
            ],
            max_help_bytes: 6000,
        },
        CliHelpEvalCase {
            task: "Discover package install, enable, disable, inspect, and set-secret under `alan q`",
            help_args: &[&["--help"], &["q", "--help"]],
            required_tokens: &[
                "install",
                "enable",
                "disable",
                "list",
                "inspect",
                "upgrade",
                "uninstall",
                "sync",
                "set-secret",
            ],
            max_help_bytes: 4000,
        },
        CliHelpEvalCase {
            task: "Discover `alan q install` flags (`--catalog-only`, `--name`)",
            help_args: &[&["q", "--help"], &["q", "install", "--help"]],
            required_tokens: &["--catalog-only", "--name", "<PATH>"],
            max_help_bytes: 3000,
        },
        CliHelpEvalCase {
            task: "Discover `alan q enable` flags (`--access`, `--allow-network`, `--skills-only`)",
            help_args: &[&["q", "--help"], &["q", "enable", "--help"]],
            required_tokens: &[
                "--access",
                "--allow-network",
                "--skills-only",
                "<PACKAGE_ID>",
            ],
            max_help_bytes: 3000,
        },
        CliHelpEvalCase {
            task: "Discover `alan q set-secret <CREDENTIAL_ID>`",
            help_args: &[&["q", "--help"], &["q", "set-secret", "--help"]],
            required_tokens: &["<CREDENTIAL_ID>", "Host Store"],
            max_help_bytes: 3000,
        },
    ];

    for case in cases {
        assert!(
            case.help_args.len() <= 2,
            "task `{}` exceeded 2-turn help discovery budget",
            case.task
        );
        let mut combined = String::new();
        for (hop, argv) in case.help_args.iter().enumerate() {
            let out = isolated_alan_cmd(root.path()).args(*argv).output().unwrap();
            assert!(
                out.status.success(),
                "task `{}` hop {hop} `{argv:?}` failed: {}",
                case.task,
                String::from_utf8_lossy(&out.stderr)
            );
            let stdout = String::from_utf8_lossy(&out.stdout);
            assert!(
                stdout.len() <= case.max_help_bytes,
                "task `{}` hop {hop} `{argv:?}` exceeded context budget ({} > {} bytes)",
                case.task,
                stdout.len(),
                case.max_help_bytes
            );
            assert!(
                !stdout.contains("--agent ") && !stdout.contains("host start"),
                "retired CLI flags leaked into `{argv:?}`:\n{stdout}"
            );
            combined.push_str(&stdout);
        }
        for token in case.required_tokens {
            assert!(
                combined.contains(token),
                "task `{}` missing required discoverability token `{token}` in:\n{combined}",
                case.task
            );
        }
    }
}
