use super::*;

#[tokio::test]
async fn test_os_backend_still_blocks_out_of_host_mount_reads() {
    // Seatbelt denies writes/network but permits reads, so the parser must still
    // contain reads that could expose host secrets to tool output.
    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for command in [
        "cat ~/.ssh/id_rsa",
        "cat /etc/passwd",
        "bash -lc 'cat /etc/passwd'",
    ] {
        let result = sandbox
            .exec_with_timeout_and_capability(
                command,
                temp.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Read),
            )
            .await;
        assert!(
            result.is_err(),
            "out-of-host_mount read not blocked: {command}"
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside host_mount"),
            "wrong rejection for: {command}"
        );
    }
}

#[tokio::test]
async fn seatbelt_rejects_makefile_commands_with_uninspectable_path_input() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let outside_file = outside.path().join("host-only-marker.txt");
    std::fs::write(&outside_file, "host-only-marker\n").unwrap();
    let makefile = format!("all:\n\tcat {}\n", outside_file.display());
    std::fs::write(mount.path().join("Makefile"), &makefile).unwrap();
    std::fs::write(mount.path().join("custom.mk"), makefile).unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for command in [
        "make",
        "make -f custom.mk",
        "gmake -f custom.mk",
        "bmake -f custom.mk",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("makefile recipes can read paths hidden from ProtectedOnly validation");

        assert!(
            error
                .to_string()
                .contains("uninspectable path-bearing input"),
            "{command}: {error}"
        );
    }
}

#[tokio::test]
async fn seatbelt_rejects_mount_local_executables_with_uninspectable_reads() {
    use std::os::unix::fs::PermissionsExt;

    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("host-only-marker.txt");
    std::fs::write(&secret, "host-only-marker\n").unwrap();
    std::fs::create_dir(mount.path().join("bin")).unwrap();
    let executable = mount.path().join("bin/leak");
    std::fs::write(
        &executable,
        format!("#!/bin/sh\n/bin/cat '{}'\n", secret.display()),
    )
    .unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    for command in [
        "./bin/leak",
        "PATH=bin leak",
        "env PATH=bin leak",
        "PATH=bin; leak",
        "export PATH=bin; leak",
    ] {
        let error = sandbox
            .exec_with_timeout_and_capability(
                command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await
            .expect_err("mount-local code can read paths hidden from ProtectedOnly validation");

        assert!(
            error.to_string().contains("mount-local executable"),
            "{command}: {error}"
        );
    }

    let non_executable = mount.path().join("bin/uname");
    std::fs::write(&non_executable, "not an executable\n").unwrap();
    std::fs::set_permissions(&non_executable, std::fs::Permissions::from_mode(0o644)).unwrap();
    let result = sandbox
        .exec_with_timeout_and_capability(
            "PATH=bin uname",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await;
    #[cfg(target_os = "macos")]
    assert_eq!(
        result
            .expect("a non-executable same-name file is not treated as the mount-local executable")
            .exit_code,
        126
    );
    #[cfg(not(target_os = "macos"))]
    assert!(
        result
            .expect_err("only macOS provides sandbox-exec for this forced backend")
            .to_string()
            .contains("Failed to execute command"),
        "a non-executable same-name file must not be rejected as a mount-local executable"
    );
}

#[tokio::test]
async fn seatbelt_rejects_git_extension_aliases_with_uninspectable_reads() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("host-only-marker.txt");
    std::fs::write(&secret, "host-only-marker\n").unwrap();
    let initialized = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(initialized.success(), "git init failed: {initialized}");
    let config_path = mount.path().join(".git/config");
    let config = std::fs::read_to_string(&config_path).unwrap();
    std::fs::write(
        &config_path,
        format!(
            "{config}\n[alias]\n\tleak = !/bin/cat '{}'\n",
            secret.display()
        ),
    )
    .unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "git leak",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Git shell aliases can execute reads hidden from ProtectedOnly validation");

    assert!(
        error.to_string().contains("opaque command dispatcher"),
        "{error}"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_rejects_git_commit_hooks_with_uninspectable_reads() {
    use std::os::unix::fs::PermissionsExt;

    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("host-only-marker.txt");
    std::fs::write(&secret, "host-only-marker\n").unwrap();
    let initialized = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(initialized.success(), "git init failed: {initialized}");
    std::fs::write(mount.path().join("tracked.txt"), "tracked\n").unwrap();
    let staged = std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(staged.success(), "git add failed: {staged}");

    let hooks = mount.path().join(".git/hooks");
    let pre_commit = hooks.join("pre-commit");
    std::fs::write(
        &pre_commit,
        format!("#!/bin/sh\ncat '{}'\n", secret.display()),
    )
    .unwrap();
    std::fs::set_permissions(&pre_commit, std::fs::Permissions::from_mode(0o755)).unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let command = "git -c user.name=Test -c user.email=test@example.invalid -c commit.gpgsign=false commit -m trigger-hook";
    let error = sandbox
        .exec_with_timeout_and_capability(
            command,
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Git commit hooks can read paths hidden from ProtectedOnly validation");

    assert!(
        error.to_string().contains("opaque command dispatcher"),
        "{error}"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_rejects_rake_project_tasks_with_uninspectable_reads() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("host-only-marker.txt");
    std::fs::write(&secret, "host-only-marker\n").unwrap();
    std::fs::write(
        mount.path().join("Rakefile"),
        format!(
            "task :leak do\n  puts File.read('{}')\nend\n",
            secret.display()
        ),
    )
    .unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "rake leak",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Rake project tasks can read paths hidden from ProtectedOnly validation");

    assert!(
        error.to_string().contains("opaque command dispatcher"),
        "{error}"
    );
}

#[tokio::test]
async fn seatbelt_rejects_go_project_runner_with_uninspectable_reads() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("host-only-marker.txt");
    std::fs::write(&secret, "host-only-marker\n").unwrap();
    std::fs::write(
        mount.path().join("main.go"),
        format!(
            "package main\nimport (\"fmt\"; \"os\")\nfunc main() {{ data, err := os.ReadFile({:?}); if err != nil {{ panic(err) }}; fmt.Print(string(data)) }}\n",
            secret.to_string_lossy()
        ),
    )
    .unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "GOCACHE=.cache go run .",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Go project code can read paths hidden from ProtectedOnly validation");

    assert!(
        error.to_string().contains("opaque command dispatcher"),
        "{error}"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_rejects_swift_project_runner_with_uninspectable_reads() {
    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("host-only-marker.txt");
    std::fs::write(&secret, "host-only-marker\n").unwrap();
    std::fs::create_dir_all(mount.path().join("Sources/leak")).unwrap();
    std::fs::write(
        mount.path().join("Package.swift"),
        "// swift-tools-version: 5.9\nimport PackageDescription\nlet package = Package(name: \"LeakFixture\", products: [.executable(name: \"leak\", targets: [\"leak\"])], targets: [.executableTarget(name: \"leak\")])\n",
    )
    .unwrap();
    std::fs::write(
        mount.path().join("Sources/leak/main.swift"),
        format!(
            "import Foundation\nlet data = try Data(contentsOf: URL(fileURLWithPath: {:?}))\nFileHandle.standardOutput.write(data)\n",
            secret.to_string_lossy()
        ),
    )
    .unwrap();

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "swift run --disable-sandbox --quiet",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Swift project code can read paths hidden from ProtectedOnly validation");

    assert!(
        error.to_string().contains("opaque command dispatcher"),
        "{error}"
    );
}

#[test]
fn project_code_dispatchers_are_rejected_as_opaque() {
    for command in [
        "git add file.txt",
        "git am change.patch",
        "git checkout branch",
        "git commit -m message",
        "git merge topic",
        "git push origin branch",
        "git rebase main",
        "git reset --hard HEAD",
        "git stash",
        "git stash push",
        "git tag v1",
        "git notes add -m note",
        "npm run leak",
        "npm --prefix . run leak",
        "npm rum leak",
        "npm urn leak",
        "npm x eslint .",
        "npm explore app -- cat /etc/passwd",
        "npm pack",
        "npx eslint .",
        "pnpm build",
        "yarn build",
        "yarn workspace app run leak",
        "yarn workspaces foreach run test",
        "bun run leak",
        "deno task leak",
        "cargo run",
        "cargo test -p app",
        "cargo --manifest-path app/Cargo.toml build",
        "cargo xtask release",
        "go run .",
        "GOCACHE=.cache go run .",
        "go -C . test ./...",
        "go generate ./...",
        "go build ./...",
        "swift build",
        "swift run",
        "swift run --disable-sandbox",
        "swift test",
        "swift --package-path . run",
        "swift package describe",
        "swift -e print(1)",
        "rake leak",
        "rake -f custom.rake leak",
        "bundle exec rake leak",
        "bundle --gemfile Gemfile exec rake leak",
        "pytest -s",
        "python3 -m pytest -s",
        "python3 -m unittest test_module",
    ] {
        let words = command
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let error = super::super::command_wrappers::validate_opaque_command_dispatchers(
            &[words],
            "test",
            true,
        )
        .expect_err(command);
        assert!(
            error.to_string().contains("opaque command dispatcher"),
            "{error}"
        );
    }

    for command in [
        "git status",
        "git -C . diff",
        "git --no-pager log",
        "git branch --list",
        "git tag --list",
        "git stash list",
        "git notes list",
        "git worktree list",
        "git remote -v",
        "npm view alan",
        "pnpm root",
        "yarn info alan",
        "cargo metadata --no-deps",
        "cargo fmt",
        "cargo new app",
        "cargo add serde",
        "go version",
        "go -C . version",
        "go env GOCACHE",
        "go doc fmt",
        "go list ./...",
        "go fmt ./...",
        "swift --version",
        "swift --help",
        "rake --version",
        "rake -V",
        "rake --help",
        "rake -H",
        "pytest --version",
    ] {
        let words = command
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        super::super::command_wrappers::validate_opaque_command_dispatchers(&[words], "test", true)
            .expect(command);
    }

    for command in [
        "cargo test",
        "pytest -q",
        "python3 -m pytest -s",
        "python3 -m unittest test_module",
        "GOCACHE=.cache go run .",
    ] {
        let words = command
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>();
        super::super::command_wrappers::validate_opaque_command_dispatchers(
            &[words],
            "test",
            false,
        )
        .expect(command);
    }
}

#[tokio::test]
async fn absolute_path_executable_on_host_path_is_not_a_project_file_operand() {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        temp.path().to_path_buf(),
        crate::tools::SandboxBackendKind::HostMountPathGuard,
    );
    tokio::fs::write(temp.path().join("inside.txt"), "inside mount")
        .await
        .unwrap();
    let search_path = std::env::var_os("PATH").expect("test PATH");
    let executable = std::env::split_paths(&search_path)
        .map(|directory| directory.join("cat"))
        .find(|path| {
            std::fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
        .expect("cat executable on PATH");

    let command = format!("'{}' inside.txt", executable.display());
    assert_eq!(sandbox.bash_path_guard_reason(&command, temp.path()), None);
    let result = sandbox
        .exec_with_timeout_and_capability(
            &command,
            temp.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Read),
        )
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    assert_eq!(result.stdout, "inside mount");
    let path_as_operand_then_command = format!(
        "cat '{}' ; '{}' inside.txt",
        executable.display(),
        executable.display()
    );
    assert!(
        sandbox
            .bash_path_guard_reason(&path_as_operand_then_command, temp.path())
            .is_some_and(|reason| reason.contains("outside host_mount"))
    );
    assert!(
        sandbox
            .bash_path_guard_reason("cat /etc/hosts", temp.path())
            .is_some_and(|reason| reason.contains("outside host_mount"))
    );
}
