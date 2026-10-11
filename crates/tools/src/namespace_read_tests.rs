use super::*;
use alan_agent_engine::{Config, tools::ToolExecutionBinding};
use alan_ap::{InProcessTransport, reference::MemFs};
use alan_kernel::{Access, MountFs, Namespace};
use std::{path::PathBuf, sync::Arc};

const OWN: &str = "/agent/8/actions/a2/output";

fn namespace(bytes: Vec<u8>) -> InProcessTransport {
    let mut namespace = Namespace::new();
    for owner in [8, 9] {
        namespace.mount(
            &format!("/agent/{owner}/actions/a2"),
            InProcessTransport::new(Arc::new(MemFs::with_read_only_files([
                ("output".into(), bytes.clone()),
                ("control".into(), b"not output".to_vec()),
            ]))),
            Access::ReadOnly,
        );
    }
    InProcessTransport::new(Arc::new(MountFs::new(namespace)))
}

fn context(bytes: Vec<u8>) -> ToolContext {
    ToolContext::from_binding(
        ToolExecutionBinding::awaiting_host_projection(PathBuf::from("/"), PathBuf::new()),
        Arc::new(Config::default()),
    )
    .with_namespace(8, namespace(bytes))
}

#[tokio::test]
async fn namespace_read_reconstructs_long_original_without_host_or_oversized_projection() {
    let raw = json!({"stdout": "字\n\u{0000}\"\\[REDACTED reason=test] server> ready a > b\n".repeat(1500),
        "stderr":"QVALUE=42137\n","exit_code":0}).to_string();
    assert!(raw.len() > 30_000);
    let ctx = context(raw.as_bytes().to_vec());
    let tool = ReadFileTool;
    let mut offset = 0;
    let mut acquired = String::new();
    loop {
        let result = tool
            .execute(json!({"path":OWN,"byte_offset":offset}), &ctx)
            .await
            .unwrap();
        assert_eq!(result["total_bytes"], raw.len());
        assert!(serde_json::to_vec(&result).unwrap().len() < 30_000);
        acquired.push_str(result["content"].as_str().unwrap());
        let next = result["next_byte_offset"].as_u64().unwrap();
        assert!(next > offset);
        offset = next;
        if result["truncated"] == false {
            break;
        }
    }
    assert_eq!(acquired, raw);
    assert!(ctx.sandbox().is_err());
}

#[tokio::test]
async fn namespace_read_rejects_cross_owner_control_alias_and_traversal() {
    let ctx = context(b"original".to_vec());
    let tool = ReadFileTool;
    for path in [
        "/agent/9/actions/a2/output",
        "/agent/root/actions/a2/output",
        "/agent/08/actions/a2/output",
        "/agent/8/actions/a2/control",
        "/agent/8/actions/../a2/output",
        "/agent/8/actions/a2/./output",
        "/agent/8/actions//a2/output",
    ] {
        assert!(
            tool.execute(json!({"path":path}), &ctx).await.is_err(),
            "{path}"
        );
    }
    assert!(
        tool.execute(json!({"path":"/agent/8/actions/missing/output"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        tool.execute(json!({"path":"/mnt/source/file"}), &ctx)
            .await
            .is_err()
    );
    assert!(
        WriteFileTool
            .execute(json!({"path":OWN,"content":"replaced"}), &ctx)
            .await
            .is_err()
    );
    let original = tool.execute(json!({"path":OWN}), &ctx).await.unwrap();
    assert_eq!(original["content"], "original");
}

#[tokio::test]
async fn namespace_read_expiry_survives_an_obsolete_offset() {
    let record =
        json!({"type":"evidence_retention_expired","reference":OWN,"cause":"storage pressure"});
    let ctx = context(record.to_string().into_bytes());
    let result = ReadFileTool
        .execute(json!({"path":OWN,"byte_offset":100_000}), &ctx)
        .await
        .unwrap();
    assert_eq!(result, record);
}

#[tokio::test]
async fn namespace_read_ranges_preserve_utf8_and_fail_closed() {
    let ctx = context("字abc".as_bytes().to_vec());
    let tool = ReadFileTool;
    let result = tool
        .execute(json!({"path":OWN,"byte_limit":4}), &ctx)
        .await
        .unwrap();
    assert_eq!(result["content"], "字a");
    assert_eq!(result["next_byte_offset"], 4);
    let result = tool
        .execute(json!({"path":OWN,"byte_offset":4}), &ctx)
        .await
        .unwrap();
    assert_eq!(result["content"], "bc");
    assert_eq!(result["truncated"], false);
    for args in [
        json!({"path":OWN,"byte_offset":1}),
        json!({"path":OWN,"byte_offset":7}),
        json!({"path":OWN,"byte_limit":1}),
        json!({"path":OWN,"byte_limit":0}),
        json!({"path":OWN,"byte_limit":4097}),
        json!({"path":OWN,"offset":1}),
    ] {
        assert!(tool.execute(args, &ctx).await.is_err());
    }
    let empty = tool
        .execute(json!({"path":OWN,"byte_offset":6}), &ctx)
        .await
        .unwrap();
    assert_eq!(empty["content"], "");
    let no_descriptor = ToolContext::from_binding(ctx.binding(), ctx.config.clone());
    assert!(
        tool.execute(json!({"path":OWN}), &no_descriptor)
            .await
            .is_err()
    );
    let clipped = context("a字bc".as_bytes().to_vec());
    let result = tool
        .execute(json!({"path":OWN,"byte_limit":2}), &clipped)
        .await
        .unwrap();
    assert_eq!(result["content"], "a");
    assert_eq!(result["next_byte_offset"], 1);
}

#[tokio::test]
async fn namespace_read_stays_inline_after_worst_case_escaping() {
    let ctx = context(vec![0; ToolContext::MAX_EVIDENCE_READ_BYTES as usize]);
    let result = ReadFileTool
        .execute(json!({"path":OWN}), &ctx)
        .await
        .unwrap();
    assert_eq!(result["content"].as_str().unwrap().len(), 4096);
    assert!(serde_json::to_vec(&result).unwrap().len() < 30_000);
}

#[tokio::test]
async fn namespace_read_process_runner_validates_schema_and_uses_the_parent() {
    use alan_agent_engine::tools::{ToolProcessInvocation, ToolRegistry};
    let mut registry = ToolRegistry::new();
    registry.register(ReadFileTool);
    let runner = registry.process_runner();
    runner.register_process_binding(8, context(b"unused".to_vec()).binding());
    for (args, expected_exit) in [
        (json!({"path":OWN,"byte_offset":2,"byte_limit":3}), 0),
        (json!({"path":OWN,"byte_limit":4097}), 1),
        (json!({"path":OWN,"byte_offset":-1}), 1),
        (json!({"path":OWN,"byte_limit":"3"}), 1),
    ] {
        let result = runner
            .run_in_namespace(
                ToolProcessInvocation {
                    pid: 20,
                    parent: Some(8),
                    executable: "/bin/read_file".into(),
                    args: vec![args.to_string()],
                },
                namespace(b"original".to_vec()),
            )
            .await;
        assert_eq!(result.exit_code, expected_exit);
        if expected_exit == 0 {
            let output: Value = serde_json::from_slice(&result.output).unwrap();
            assert_eq!(output["content"], "igi");
        }
    }
}
