#![cfg(target_os = "macos")]

use super::super::*;
use crate::tools::{SandboxBackendKind, reified_namespace::ReifiedMountAccess};
use std::path::PathBuf;
use tempfile::TempDir;

#[tokio::test]
async fn sandbox_runs_awk_with_native_paths_and_preserves_path_data() {
    let mount = TempDir::new().unwrap();
    let input_path = mount.path().join("input.tsv");
    let output_path = mount.path().join("output.tsv");
    std::fs::write(&input_path, "payload\n").unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "awk '{{ print $0 }}' '{}' > '{}'",
                input_path.display(),
                output_path.display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(std::fs::read_to_string(&output_path).unwrap(), "payload\n");

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "env -u HOME awk -v root=/mnt/project 'BEGIN {{ print root }}' > '{}'",
                mount.path().join("data.txt").display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(
        std::fs::read_to_string(mount.path().join("data.txt")).unwrap(),
        "/mnt/project\n"
    );

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "awk '{{ print root }}' root=/mnt/project '{}'",
                input_path.display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "/mnt/project\n");

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!("awk '/payload/ {{ print $0 }}' '{}'", input_path.display()),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "payload\n");

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "awk '$0 ~ /getline < p/ {{ print }}' '{}'",
                input_path.display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert!(result.stdout.is_empty());

    let result = sandbox
        .exec_with_timeout_and_capability(
            &format!(
                "awk 'BEGIN {{ if (getline && NR < 3) print NR }}' < '{}'",
                input_path.display()
            ),
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "1\n");

    let result = sandbox
        .exec_with_timeout_and_capability(
            "awk 'BEGIN { while ((getline line < \"input.tsv\") > 0) print line }'",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert_eq!(result.stdout, "payload\n");
}

#[tokio::test]
async fn sandbox_rejects_awk_program_reads_outside_the_host_mount() {
    let mount = TempDir::new().unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    for command in [
        "awk 'BEGIN { while ((getline x < \"/etc/passwd\") > 0) print x }'",
        "env -u HOME gawk 'BEGIN { while ((getline x < \"/etc/hosts\") > 0) print x }'",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("AWK program paths outside the grant must be rejected before execution");
        assert!(error.to_string().contains("outside host_mount"), "{error}");
    }
}

#[tokio::test]
async fn sandbox_rejects_dynamic_awk_getline_paths() {
    let mount = TempDir::new().unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    for command in [
        "awk -v p=/etc/passwd 'BEGIN { getline x < p; print x }'",
        "awk -v p=/etc/passwd 'BEGIN { getline $10 < p; print $10 }'",
        "awk 'BEGIN { getline x < \"\\057etc/passwd\"; print x }'",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("dynamic or escaped AWK file paths must fail closed");

        assert!(
            error
                .to_string()
                .contains("AWK getline file paths unless they are simple"),
            "{error}"
        );
    }

    let error = sandbox
        .exec_with_timeout_and_capability(
            "awk 'BEGIN { getline x < \"../../../../../../etc/passwd\"; print x }'",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("relative AWK file paths outside the Host Mount must be rejected");
    assert!(error.to_string().contains("outside host_mount"), "{error}");
}

#[tokio::test]
async fn sandbox_rejects_opaque_awk_program_files_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    let script_path = mount.path().join("script.awk");
    std::fs::write(
        &script_path,
        "BEGIN { getline x < \"/etc/passwd\"; print x }\n",
    )
    .unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    for command in [
        "awk -f script.awk",
        "gawk -i script.awk",
        "gawk --include=script.awk",
        "gawk -i /etc/evil.awk 'BEGIN {}'",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("opaque AWK script files cannot be checked against the Host Mount");

        assert!(
            error.to_string().contains("opaque AWK script files"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn sandbox_rejects_nested_dispatcher_reads_outside_the_host_mount_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let error = sandbox
        .exec_with_timeout_and_capability(
            "printf '/etc/passwd\\n' | xargs cat",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Seatbelt commands must not dispatch unchecked reads outside Host Mounts");

    assert!(
        error
            .to_string()
            .contains("rejects opaque command dispatchers like xargs"),
        "{error}"
    );
}

#[tokio::test]
async fn sandbox_rejects_uninspectable_path_inputs_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("host-only-marker.txt");
    std::fs::write(&outside_file, "host-only-marker\n").unwrap();
    std::fs::create_dir(mount.path().join("pax-copy-destination")).unwrap();
    std::fs::write(
        mount.path().join("curl.conf"),
        format!("url = \"file://{}\"\n", outside_file.display()),
    )
    .unwrap();
    std::fs::write(
        mount.path().join("sed.rules"),
        format!("r {}\n", outside_file.display()),
    )
    .unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let commands = [
        format!(
            "printf '{}\\n' | tar -cf - -T - | tar -xOf -",
            outside_file.display()
        ),
        format!(
            "printf '{}\\n' | tar -cf - --files-from - | tar -xOf -",
            outside_file.display()
        ),
        format!(
            "printf '{}\\n' | zip -q -@ host-files.zip && unzip -p host-files.zip",
            outside_file.display()
        ),
        format!(
            "printf '{}\\n' | zip -q@ host-files.zip && unzip -p host-files.zip",
            outside_file.display()
        ),
        format!("printf '{}\\n' | cpio -o | strings", outside_file.display()),
        format!("printf '{}\\n' | cpio -pd .", outside_file.display()),
        format!(
            "printf '{}\\n' | pax -w -f host-files.pax",
            outside_file.display()
        ),
        format!(
            "printf '{}\\n' | pax -rw pax-copy-destination",
            outside_file.display()
        ),
        format!(
            "printf 'url = \"file://{}\"\\n' | curl -q --config -",
            outside_file.display()
        ),
        "curl -q -K curl.conf".to_string(),
        "printf 'x\\n' | sed -n -f sed.rules".to_string(),
    ];
    for command in commands {
        let error = sandbox
            .exec_with_timeout_and_capability(
                &command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("uninspectable path inputs must not bypass Host Mount read checks");

        assert!(
            error
                .to_string()
                .contains("uninspectable path-bearing input"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn sandbox_allows_pax_write_with_explicit_file_operands_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    std::fs::write(mount.path().join("input.txt"), "payload\n").unwrap();
    std::fs::create_dir(mount.path().join("copy-destination")).unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let result = sandbox
        .exec_with_timeout_and_capability(
            "pax -w -f host-files.pax input.txt",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert!(mount.path().join("host-files.pax").is_file());

    let result = sandbox
        .exec_with_timeout_and_capability(
            "pax -rw input.txt copy-destination",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await
        .unwrap();

    assert_eq!(result.exit_code, 0, "{}", result.stderr);
    assert!(mount.path().join("copy-destination/input.txt").is_file());
}

#[tokio::test]
async fn sandbox_rejects_opaque_python_reads_outside_the_host_mount_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let error = sandbox
        .exec_with_timeout_and_capability(
            "python3 -c 'print(open(chr(47) + \"etc/passwd\").read())'",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("opaque Python evaluators must be rejected before Seatbelt execution");
    assert!(
        error
            .to_string()
            .contains("rejects nested command evaluators"),
        "{error}"
    );
}

#[tokio::test]
async fn sandbox_rejects_awk_program_command_execution_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    for command in [
        "awk 'BEGIN { system(\"cat \" sprintf(\"%c\", 47) \"etc/passwd\") }'",
        "awk 'BEGIN { \"cat \" sprintf(\"%c\", 47) \"etc/passwd\" | getline line; print line }'",
        "awk 'BEGIN { ARGV[1] = sprintf(\"%cetc/passwd\", 47); ARGC = 2 } { print }'",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("opaque AWK I/O must be rejected before Seatbelt execution");
        assert!(error.to_string().contains("AWK"), "{error}");
    }
}

#[tokio::test]
async fn sandbox_rejects_awk_program_writes_to_protected_subpaths_under_seatbelt() {
    let mount = TempDir::new().unwrap();
    let git_dir = mount.path().join(".git");
    std::fs::create_dir(&git_dir).unwrap();
    let git_config = git_dir.join("config");
    std::fs::write(&git_config, "preserve\n").unwrap();
    let spec = SandboxSpec::from_host_mounts(&[SandboxHostMount {
        namespace_path: PathBuf::from("/mnt/project"),
        host_path: mount.path().to_path_buf(),
        access: ReifiedMountAccess::ReadWrite,
    }]);
    let sandbox = Sandbox::from_spec_with_backend(spec, SandboxBackendKind::Seatbelt);

    let result = sandbox
        .exec_with_timeout_and_capability(
            "awk 'BEGIN { print \"overwrite\" > \".git/config\" }'",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Write),
        )
        .await;

    assert!(
        result.is_err(),
        "AWK must not bypass the protected path guard"
    );
    assert_eq!(std::fs::read_to_string(git_config).unwrap(), "preserve\n");
}
