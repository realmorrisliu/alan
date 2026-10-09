//! Opt-in acceptance client for a freshly launched native Alan Root in Herdr.
use alan_agent_engine::OwnerWorkControl;
use alan_os_host::{HostEndpointPaths, LocalAttachment};
use alan_shell::Shell;
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

#[tokio::test]
#[ignore = "requires explicitly selected live Alan instance and a frozen authorized work control"]
async fn native_root_completes_explicit_owner_work_without_assistant_output() {
    let runtime =
        PathBuf::from(std::env::var_os("ALAN_INSTANCE_RUNTIME_DIR").expect("explicit instance"));
    let control_path =
        PathBuf::from(std::env::var_os("ALAN_OWNER_WORK_CONTROL").expect("frozen control"));
    let evidence_dir = PathBuf::from(
        std::env::var_os("ALAN_OWNER_WORK_EVIDENCE_DIR").expect("evidence outside project"),
    );
    let control: OwnerWorkControl =
        serde_json::from_slice(&std::fs::read(control_path).unwrap()).unwrap();
    let paths = HostEndpointPaths::from_runtime_dir(&runtime).unwrap();
    let attached = LocalAttachment::new(paths.clone()).connect().await.unwrap();
    let shell = Shell::new(attached.root);
    let output_before = shell.cat("/agent/root/io/output").await.unwrap();
    if let Some(project) = std::env::var_os("ALAN_OWNER_WORK_REAUTHORIZE_PROJECT") {
        let root_pid = String::from_utf8(
            shell
                .cat("/mnt/service-manager/units/root-agent/pid")
                .await
                .unwrap(),
        )
        .unwrap();
        let namespace_path = format!("/proc/{}/namespace", root_pid.trim());
        let before = String::from_utf8(shell.cat(&namespace_path).await.unwrap()).unwrap();
        assert!(
            !before
                .lines()
                .any(|line| line.starts_with("/mnt/project-request-")),
            "recovery must not restore Host authority"
        );
        let status = paths.read_status().unwrap();
        let host = alan_os_host::HostCommandPlane::new(paths);
        let mounted = host
            .mount_project(
                uuid::Uuid::new_v4(),
                status.boot_id,
                PathBuf::from(project),
                alan_service_manager::HostMountAccess::ReadOnly,
            )
            .await
            .unwrap();
        assert_eq!(mounted.grant.namespace_path, "/mnt/project-request-1");
        let after = String::from_utf8(shell.cat(&namespace_path).await.unwrap()).unwrap();
        assert!(
            after
                .lines()
                .any(|line| line == "/mnt/project-request-1 ro")
        );
        std::fs::write(
            evidence_dir.join("reauthorized-grant.json"),
            serde_json::to_vec_pretty(&mounted.grant).unwrap(),
        )
        .unwrap();
    }

    let expected =
        std::env::var("ALAN_OWNER_WORK_EXPECTED_STATE").unwrap_or_else(|_| "completed".into());
    assert!(matches!(
        expected.as_str(),
        "completed" | "waiting" | "failed"
    ));
    if std::env::var("ALAN_OWNER_WORK_OBSERVE_ONLY").as_deref() != Ok("1") {
        shell
            .write(
                "/agent/root/machine/ctl",
                control.encode().unwrap().as_bytes(),
            )
            .await
            .unwrap();
    }
    let result = tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let value: Value =
                serde_json::from_slice(&shell.cat("/agent/root/machine/work").await.unwrap())
                    .unwrap();
            if value["work"]["work_id"] == control.id.to_string()
                && (value["work"]["state"] == expected
                    || matches!(
                        value["work"]["state"].as_str(),
                        Some("completed" | "failed" | "cancelled" | "interrupted")
                    ))
            {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("native bounded work observation");
    let evaluation = shell.cat("/agent/root/machine/evaluation").await.unwrap();
    std::fs::write(
        evidence_dir.join("work.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    std::fs::write(evidence_dir.join("evaluation.json"), evaluation).unwrap();
    assert_eq!(result["work"]["state"], expected, "{result}");
    if expected != "completed" {
        assert!(result["work"]["owner"].is_null());
        assert_eq!(result["work"]["generation_calls"], 0);
        if expected == "waiting" {
            let request_id = result["work"]["pending_request"]["id"].as_str().unwrap();
            assert_eq!(
                shell
                    .cat(&format!("/agent/root/requests/{request_id}/status"))
                    .await
                    .unwrap(),
                b"pending"
            );
        }
        assert_eq!(
            shell.cat("/agent/root/io/output").await.unwrap(),
            output_before
        );
        std::fs::write(
            evidence_dir.join("activity.json"),
            shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
        )
        .unwrap();
        return;
    }
    assert_eq!(result["work"]["owner"], "hostfs");
    assert_eq!(result["work"]["generation_calls"], 0);
    assert!(
        result["work"]["citations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["path"]
                .as_str()
                .unwrap()
                .starts_with("/mnt/project-request-1/")
                && c["sha256"].as_str().unwrap().len() == 64)
    );
    assert_eq!(
        shell.cat("/agent/root/io/output").await.unwrap(),
        output_before
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let activity: Value = serde_json::from_slice(
                &shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
            )
            .unwrap();
            if activity["state"] == "idle" {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("completed work settles its UI activity without ending the Process");
    std::fs::write(
        evidence_dir.join("activity.json"),
        shell.cat("/agent/root/machine/ui/activity").await.unwrap(),
    )
    .unwrap();
}
