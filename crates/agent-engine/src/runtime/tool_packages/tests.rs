use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::tools::{ToolContext, ToolResult, ToolSecretResolver};

struct ExampleTool;

impl Tool for ExampleTool {
    fn name(&self) -> &str {
        "example"
    }
    fn description(&self) -> &str {
        "Example Tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolResult {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }
}

struct ArgumentDependentTool;

impl Tool for ArgumentDependentTool {
    fn name(&self) -> &str {
        "dynamic"
    }
    fn description(&self) -> &str {
        "Dynamic Tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    fn execute(&self, _: serde_json::Value, _: &ToolContext) -> ToolResult {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }
    fn capability_is_argument_dependent(&self) -> bool {
        true
    }
}

#[derive(Debug, Default)]
struct MockSecretResolver {
    secrets: Mutex<BTreeMap<String, String>>,
}

impl MockSecretResolver {
    fn with_secret(self, id: &str, val: &str) -> Self {
        self.secrets
            .lock()
            .unwrap()
            .insert(id.to_string(), val.to_string());
        self
    }
}

impl ToolSecretResolver for MockSecretResolver {
    fn resolve_named_secret(&self, credential_id: &str) -> Result<Option<String>> {
        Ok(self.secrets.lock().unwrap().get(credential_id).cloned())
    }
}

fn argv_manifest(name: &str) -> ToolPackageManifest {
    ToolPackageManifest {
        version: ToolPackageManifest::schema_version(),
        name: name.to_string(),
        description: "Argv test tool".to_string(),
        parameters: default_argv_parameters_schema(),
        capability: ToolCapability::Unknown,
        capability_is_argument_dependent: false,
        timeout_secs: 15,
        execution: ToolExecutionHints {
            arguments: TOOL_ARGUMENTS_ARGV.to_string(),
            result: TOOL_RESULT_STDOUT_JSON.to_string(),
        },
        network_domains: Vec::new(),
        required_secrets: Vec::new(),
        required_adc: None,
    }
}

#[cfg(unix)]
fn write_executable_script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn manifest_round_trips_and_validates_against_mount_name() {
    let manifest = ToolPackageManifest::from_tool(&ExampleTool, 30).unwrap();
    let bytes = serde_json::to_vec(&manifest).unwrap();
    let decoded: ToolPackageManifest = serde_json::from_slice(&bytes).unwrap();
    decoded.validate_for_name("example").unwrap();
    assert!(decoded.validate_for_name("other").is_err());

    // Legacy JSON without network_domains/required_secrets/required_adc still deserializes.
    let legacy_json = serde_json::json!({
        "version": 1,
        "name": "example",
        "description": "Legacy",
        "parameters": {"type": "object"},
        "capability": "read",
        "timeout_secs": 30,
        "execution": { "arguments": "json_first_arg", "result": "stdout_json" }
    });
    let legacy: ToolPackageManifest = serde_json::from_value(legacy_json).unwrap();
    legacy.validate_for_name("example").unwrap();
    assert!(legacy.network_domains.is_empty());
    assert!(legacy.required_secrets.is_empty());
    assert!(legacy.required_adc.is_none());
}

#[test]
fn manifest_accepts_zero_as_the_no_timeout_sentinel() {
    ToolPackageManifest::from_tool(&ExampleTool, 0).unwrap();
}

#[test]
fn argument_dependent_manifest_never_reuses_null_argument_capability() {
    let manifest = ToolPackageManifest::from_tool(&ArgumentDependentTool, 30).unwrap();
    assert!(manifest.capability_is_argument_dependent);
}

#[test]
fn schema_validation_rejects_invalid_and_non_object_schemas() {
    assert!(validate_tool_parameters_schema(&serde_json::json!("not-an-object")).is_err());
    assert!(
        validate_tool_parameters_schema(&serde_json::json!({
            "type": "object",
            "properties": { "q": { "type": "string" } }
        }))
        .is_ok()
    );
}

#[test]
fn secret_ref_and_network_domain_validation_fail_closed() {
    assert!(
        validate_tool_secret_ref(
            &ToolSecretRef {
                env: "GEMINI_API_KEY".to_string(),
                credential_id: "gemini-api-key".to_string(),
            },
            "required_secrets"
        )
        .is_ok()
    );
    // Reject raw credential values, uppercase tokens, paths, and reserved env vars.
    for bad in [
        ToolSecretRef {
            env: "GEMINI_API_KEY".to_string(),
            credential_id: "sk-1234567890".to_string(),
        },
        ToolSecretRef {
            env: "GEMINI_API_KEY".to_string(),
            credential_id: "AIzaSyFakeKeyValue".to_string(),
        },
        ToolSecretRef {
            env: "GEMINI_API_KEY".to_string(),
            credential_id: "/Users/me/.config/adc.json".to_string(),
        },
        ToolSecretRef {
            env: "PATH".to_string(),
            credential_id: "gemini-api-key".to_string(),
        },
        ToolSecretRef {
            env: "ALAN_INTERNAL".to_string(),
            credential_id: "gemini-api-key".to_string(),
        },
    ] {
        assert!(
            validate_tool_secret_ref(&bad, "required_secrets").is_err(),
            "expected rejection for {bad:?}"
        );
    }

    assert_eq!(
        validate_network_domain("generativelanguage.googleapis.com").unwrap(),
        "generativelanguage.googleapis.com"
    );
    assert!(validate_network_domain("https://generativelanguage.googleapis.com").is_err());
    assert!(validate_network_domain("*.googleapis.com").is_err());
    assert!(validate_network_domain("").is_err());
}

#[test]
fn packaged_tool_new_rejects_network_domain_mismatches() {
    let temp = tempfile::tempdir().unwrap();
    let script = temp.path().join("bin.sh");
    std::fs::write(&script, "#!/bin/sh\n").unwrap();

    // allow_network = true with empty network_domains fails closed.
    let mut manifest = argv_manifest("net_tool");
    manifest.capability = ToolCapability::Network;
    assert!(PackagedTool::new(manifest.clone(), script.clone(), true).is_err());

    // allow_network = false with non-empty network_domains fails closed.
    manifest.network_domains = vec!["generativelanguage.googleapis.com".to_string()];
    assert!(PackagedTool::new(manifest.clone(), script.clone(), false).is_err());

    // allow_network = true with valid network_domains succeeds.
    assert!(PackagedTool::new(manifest, script, true).is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn argv_forwarding_passes_raw_args_and_stdin_without_shell_expansion() {
    let temp = tempfile::tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let script = write_executable_script(
        temp.path(),
        "argv_echo.py",
        r#"#!/usr/bin/env python3
import json, sys
print(json.dumps({"argv": sys.argv[1:], "stdin": sys.stdin.read()}))
"#,
    );

    let tool = PackagedTool::new(argv_manifest("argv_echo"), script, false).unwrap();
    let binding =
        crate::tools::ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), scratch);
    let ctx = ToolContext::from_binding(binding, Arc::new(crate::Config::default()));

    // Shell metacharacters must pass verbatim as argv elements, never evaluated by sh -c.
    let result = tool
        .execute(
            serde_json::json!({
                "args": ["--flag", "$(echo pwned)", "; touch /tmp/nope", "hello world"],
                "stdin": "line1\nline2"
            }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(
        result["argv"],
        serde_json::json!([
            "--flag",
            "$(echo pwned)",
            "; touch /tmp/nope",
            "hello world"
        ])
    );
    assert_eq!(result["stdin"], serde_json::json!("line1\nline2"));
}

#[cfg(unix)]
#[tokio::test]
async fn packaged_tool_injects_secrets_and_ephemeral_0600_adc_and_cleans_up_without_leaks() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let script = write_executable_script(
        temp.path(),
        "inspect_env.py",
        r#"#!/usr/bin/env python3
import json, os, stat, sys
adc_path = os.environ.get("GOOGLE_APPLICATION_CREDENTIALS", "")
adc_mode = stat.S_IMODE(os.stat(adc_path).st_mode) if adc_path else None
adc_body = open(adc_path, "r").read() if adc_path else ""
if "--fail-with-leak" in sys.argv[1:]:
    sys.stderr.write(f"failed using key={os.environ.get('GEMINI_API_KEY')} refresh=super-secret-refresh-token\n")
    sys.exit(2)
print(json.dumps({
    "has_key": os.environ.get("GEMINI_API_KEY") == "test-gemini-key-value",
    "adc_path": adc_path,
    "adc_mode": oct(adc_mode) if adc_mode is not None else None,
    "adc_parsed": json.loads(adc_body)["type"],
}))
"#,
    );

    let mut manifest = argv_manifest("secret_tool");
    manifest.required_secrets = vec![ToolSecretRef {
        env: "GEMINI_API_KEY".to_string(),
        credential_id: "gemini-api-key".to_string(),
    }];
    manifest.required_adc = Some(ToolSecretRef {
        env: "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
        credential_id: "colab-adc".to_string(),
    });

    // Missing secret fails deterministically with actionable setup hint and zero leaks.
    let missing_resolver = Arc::new(MockSecretResolver::default());
    let missing_tool = PackagedTool::new(manifest.clone(), script.clone(), false)
        .unwrap()
        .with_secret_resolver(missing_resolver);
    let binding = crate::tools::ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/"),
        scratch.clone(),
    );
    let ctx = ToolContext::from_binding(binding, Arc::new(crate::Config::default()));
    let err = missing_tool
        .execute(serde_json::json!({ "args": [] }), &ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("gemini-api-key"), "{err}");
    assert!(err.contains("alan q set-secret gemini-api-key"), "{err}");

    // Populated resolver injects env var + ephemeral 0600 ADC file, then deletes ADC file.
    let adc_json = serde_json::json!({
        "type": "authorized_user",
        "client_id": "cid",
        "client_secret": "super-client-secret",
        "refresh_token": "super-secret-refresh-token"
    })
    .to_string();
    let resolver = Arc::new(
        MockSecretResolver::default()
            .with_secret("gemini-api-key", "test-gemini-key-value")
            .with_secret("colab-adc", &adc_json),
    );
    let tool = PackagedTool::new(manifest, script, false)
        .unwrap()
        .with_secret_resolver(resolver);
    let res = tool
        .execute(serde_json::json!({ "args": [] }), &ctx)
        .await
        .unwrap();
    assert_eq!(res["has_key"], serde_json::json!(true));
    assert_eq!(res["adc_mode"], serde_json::json!("0o600"));
    assert_eq!(res["adc_parsed"], serde_json::json!("authorized_user"));
    let adc_path = PathBuf::from(res["adc_path"].as_str().unwrap());
    assert!(
        !adc_path.exists(),
        "ephemeral ADC file must be removed on completion"
    );

    // Failing execution must scrub injected secret values from stderr error messages.
    let fail_err = tool
        .execute(serde_json::json!({ "args": ["--fail-with-leak"] }), &ctx)
        .await
        .unwrap_err()
        .to_string();
    assert!(fail_err.contains("[REDACTED]"), "{fail_err}");
    assert!(!fail_err.contains("test-gemini-key-value"), "{fail_err}");
    assert!(
        !fail_err.contains("super-secret-refresh-token"),
        "{fail_err}"
    );
    assert_eq!(
        std::fs::metadata(&scratch).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[cfg(unix)]
#[tokio::test]
async fn confined_tool_network_proxy_allows_listed_domain_and_blocks_unlisted_egress() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((mut sock, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                let _ = sock.read(&mut buf).await;
                let body = r#"{"server":"ok"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });

    let temp = tempfile::tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let script = write_executable_script(
        temp.path(),
        "net_probe.py",
        r#"#!/usr/bin/env python3
import json, sys, urllib.request, urllib.error, socket

mode = sys.argv[1]
url = sys.argv[2]
if mode == "http":
    try:
        with urllib.request.urlopen(url, timeout=3) as resp:
            print(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        print(json.dumps({"http_status": e.code, "policy": e.headers.get("X-Alan-Network-Policy")}))
elif mode == "raw":
    host, port = url.split(":")
    s = socket.socket()
    try:
        s.connect((host, int(port)))
        print(json.dumps({"raw_connected": True}))
    except Exception as e:
        print(json.dumps({"raw_connected": False, "error": str(e)}))
"#,
    );

    let mut manifest = argv_manifest("net_probe");
    manifest.capability = ToolCapability::Network;
    manifest.network_domains = vec!["127.0.0.1".to_string()];

    let tool = PackagedTool::new(manifest, script, true).unwrap();
    let binding =
        crate::tools::ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), scratch);
    let ctx = ToolContext::from_binding(binding, Arc::new(crate::Config::default()));

    // 1. Listed domain (`127.0.0.1`) succeeds via proxy.
    let ok = tool
        .execute(
            serde_json::json!({ "args": ["http", format!("http://127.0.0.1:{}/ping", addr.port())] }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(ok["server"], serde_json::json!("ok"));

    // 2. Unlisted domain (`localhost` != `127.0.0.1` or `unlisted.example`) is rejected with 403 domain-denied.
    let denied = tool
        .execute(
            serde_json::json!({ "args": ["http", "http://unlisted.example/secret"] }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(denied["http_status"], serde_json::json!(403));
    assert_eq!(denied["policy"], serde_json::json!("domain-denied"));

    // 3. On macOS Seatbelt, raw socket directly to the test server port bypassing the proxy is blocked by kernel.
    if crate::tools::detect_backend() == crate::tools::SandboxBackendKind::Seatbelt {
        let raw_blocked = tool
            .execute(
                serde_json::json!({ "args": ["raw", format!("127.0.0.1:{}", addr.port())] }),
                &ctx,
            )
            .await
            .unwrap();
        assert_eq!(raw_blocked["raw_connected"], serde_json::json!(false));
    }
}

#[cfg(all(target_os = "macos", unix))]
#[tokio::test]
async fn confined_tool_blocks_ambient_keychain_reads() {
    let temp = tempfile::tempdir().unwrap();
    let scratch = temp.path().join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let fake_home = temp.path().join("fake-home");
    let keychain_dir = fake_home.join("Library/Keychains");
    std::fs::create_dir_all(&keychain_dir).unwrap();
    let keychain_file = keychain_dir.join("login.keychain-db");
    std::fs::write(&keychain_file, b"secret-keychain-bytes").unwrap();

    let script = write_executable_script(
        temp.path(),
        "keychain_probe.py",
        r#"#!/usr/bin/env python3
import json, sys
try:
    open(sys.argv[1], "rb").read()
    print(json.dumps({"read_allowed": True}))
except PermissionError as e:
    print(json.dumps({"read_allowed": False, "errno": e.errno}))
"#,
    );

    let tool = PackagedTool::new(argv_manifest("keychain_probe"), script, false).unwrap();
    let mut spec = SandboxSpec::seed(scratch.clone());
    spec.read_denylist = SandboxSpec::sensitive_read_denylist_for_home(&fake_home);
    let sandbox = Sandbox::from_spec(spec);

    #[derive(Debug)]
    struct CustomSandboxAdapter {
        cwd: PathBuf,
        sandbox: Sandbox,
    }
    impl crate::tools::ToolExecutionAdapter for CustomSandboxAdapter {
        fn namespace_cwd(&self) -> PathBuf {
            PathBuf::from("/mnt/scratch")
        }
        fn cwd(&self) -> Result<PathBuf> {
            Ok(self.cwd.clone())
        }
        fn resolve_path(&self, _: &Path, path: &Path) -> Result<PathBuf> {
            Ok(self.cwd.join(path))
        }
        fn visible_path(&self, host_path: &Path) -> PathBuf {
            host_path.to_path_buf()
        }
        fn project_text(&self, text: &str) -> String {
            text.to_string()
        }
        fn sandbox(&self) -> Result<Sandbox> {
            Ok(self.sandbox.clone())
        }
    }

    let binding = crate::tools::ToolExecutionBinding::awaiting_host_projection(
        PathBuf::from("/mnt/scratch"),
        scratch.clone(),
    )
    .with_adapter(Arc::new(CustomSandboxAdapter {
        cwd: scratch,
        sandbox,
    }));
    let ctx = ToolContext::from_binding(binding, Arc::new(crate::Config::default()));
    let res = tool
        .execute(
            serde_json::json!({ "args": [keychain_file.display().to_string()] }),
            &ctx,
        )
        .await
        .unwrap();
    assert_eq!(res["read_allowed"], serde_json::json!(false));
}
