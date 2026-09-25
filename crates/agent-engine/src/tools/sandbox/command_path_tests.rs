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
async fn seatbelt_does_not_allow_git_configured_helpers_to_read_outside_the_host_mount() {
    use std::os::unix::fs::PermissionsExt;

    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let marker = outside.path().join("host-only-marker.txt");
    std::fs::write(&marker, "host-only-marker\n").unwrap();
    let initialized = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(initialized.success(), "git init failed: {initialized}");

    let tracked = mount.path().join("tracked.txt");
    std::fs::write(&tracked, "before\n").unwrap();
    let staged = std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(staged.success(), "git add failed: {staged}");
    std::fs::write(&tracked, "after\n").unwrap();

    let fsmonitor = mount.path().join("fsmonitor");
    std::fs::write(
        &fsmonitor,
        format!(
            "#!/bin/sh\n/bin/cat '{}' >&2\nprintf '1\\n\\0'\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fsmonitor, std::fs::Permissions::from_mode(0o755)).unwrap();
    let configured = std::process::Command::new("git")
        .args(["config", "core.fsmonitor"])
        .arg(&fsmonitor)
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(configured.success(), "git config failed: {configured}");
    let diff_configured = std::process::Command::new("git")
        .args(["config", "diff.external"])
        .arg(&fsmonitor)
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(
        diff_configured.success(),
        "git diff config failed: {diff_configured}"
    );

    let control = std::process::Command::new("git")
        .args(["status", "--short"])
        .current_dir(mount.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&control.stderr).contains("host-only-marker"),
        "fixture Git did not invoke fsmonitor: {control:?}"
    );
    let diff_control = std::process::Command::new("git")
        .args(["diff", "--", "tracked.txt"])
        .current_dir(mount.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&diff_control.stderr).contains("host-only-marker"),
        "fixture Git did not invoke diff.external: {diff_control:?}"
    );

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let commands = [
        ("git status --short".to_string(), true),
        ("git diff -- tracked.txt".to_string(), false),
        (
            "git diff --no-ext-diff --no-textconv -- tracked.txt".to_string(),
            true,
        ),
        (
            format!(
                "git -c core.fsmonitor={} status --short",
                fsmonitor.display()
            ),
            false,
        ),
        (
            format!(
                "GIT_CONFIG_COUNT=0 GIT_CONFIG_KEY_0=core.fsmonitor GIT_CONFIG_VALUE_0='{}' git status --short",
                fsmonitor.display()
            ),
            false,
        ),
        ("env -i git status --short".to_string(), false),
        ("env - git status --short".to_string(), false),
        ("exec -c git status --short".to_string(), false),
        (
            "env -u GIT_CONFIG_COUNT git status --short".to_string(),
            false,
        ),
    ];
    for (command, should_run) in commands {
        let result = sandbox
            .exec_with_timeout_and_capability(
                &command,
                mount.path(),
                None,
                Some(alan_agent_protocol::ToolCapability::Unknown),
            )
            .await;

        match result {
            Ok(output) => {
                assert!(should_run, "unsafe Git command was allowed: {command}");
                assert!(
                    !output.stderr.contains("host-only-marker"),
                    "Git configured helper read outside the active Host Mount for {command}: {output:?}"
                );
                if command == "git status --short" {
                    assert_eq!(output.exit_code, 0, "Git status failed: {output:?}");
                } else {
                    assert!(
                        output.stdout.contains("-before") && output.stdout.contains("+after"),
                        "disabling external helpers must retain Git's built-in diff: {output:?}"
                    );
                }
            }
            Err(error) => {
                assert!(
                    !should_run,
                    "safe Git command was rejected for {command}: {error}"
                );
                assert!(
                    error.to_string().contains("opaque command dispatcher")
                        || error
                            .to_string()
                            .contains("Git config environment override"),
                    "unexpected sandbox rejection for {command}: {error}"
                );
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_rejects_git_help_configured_man_viewers() {
    use std::os::unix::fs::PermissionsExt;

    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let marker = outside.path().join("host-only-marker.txt");
    std::fs::write(&marker, "host-only-marker\n").unwrap();
    let initialized = std::process::Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(mount.path())
        .status()
        .unwrap();
    assert!(initialized.success(), "git init failed: {initialized}");

    let viewer = mount.path().join("viewer");
    std::fs::write(
        &viewer,
        format!("#!/bin/sh\n/bin/cat '{}' >&2\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&viewer, std::fs::Permissions::from_mode(0o755)).unwrap();
    for (key, value) in [
        ("man.viewer", "leak"),
        ("man.leak.cmd", viewer.to_str().unwrap()),
    ] {
        let configured = std::process::Command::new("git")
            .args(["config", key, value])
            .current_dir(mount.path())
            .status()
            .unwrap();
        assert!(
            configured.success(),
            "git config {key} failed: {configured}"
        );
    }

    let control = std::process::Command::new("git")
        .args(["help", "status"])
        .current_dir(mount.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&control.stderr).contains("host-only-marker"),
        "fixture Git did not invoke the configured man viewer: {control:?}"
    );

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "git help status",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Git help can execute a configured man viewer outside the mount");
    assert!(
        error.to_string().contains("opaque command dispatcher"),
        "{error}"
    );
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn seatbelt_rejects_cmake_project_code_with_uninspectable_reads() {
    let cmake = std::process::Command::new("cmake")
        .arg("--version")
        .output()
        .expect("macOS test runner provides CMake");
    assert!(cmake.status.success(), "cmake --version failed: {cmake:?}");

    let mount = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let marker = outside.path().join("host-only-marker.txt");
    std::fs::write(&marker, "host-only-marker\n").unwrap();
    std::fs::write(
        mount.path().join("CMakeLists.txt"),
        format!(
            "cmake_minimum_required(VERSION 3.20)\nproject(leak NONE)\nexecute_process(COMMAND /bin/cat \"{}\" OUTPUT_VARIABLE secret)\nmessage(STATUS \"${{secret}}\")\n",
            marker.display()
        ),
    )
    .unwrap();

    let control = std::process::Command::new("cmake")
        .args(["-S", ".", "-B", "build"])
        .current_dir(mount.path())
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&control.stdout).contains("host-only-marker"),
        "fixture CMake did not execute the project command: {control:?}"
    );

    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "cmake -S . -B build",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("CMake project code can read paths hidden from ProtectedOnly validation");
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
async fn seatbelt_rejects_git_config_reads_outside_the_mount() {
    let mount = TempDir::new().unwrap();
    let sandbox = Sandbox::with_backend(
        mount.path().to_path_buf(),
        crate::tools::SandboxBackendKind::Seatbelt,
    );
    let error = sandbox
        .exec_with_timeout_and_capability(
            "git config --global --list",
            mount.path(),
            None,
            Some(alan_agent_protocol::ToolCapability::Unknown),
        )
        .await
        .expect_err("Git global configuration is outside the active Host Mount");

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
        "git config --global --list",
        "git config --system --list",
        "git config --list",
        "git -c core.fsmonitor=./leak status",
        "git --config-env=core.fsmonitor=GIT_CONFIG_VALUE status",
        "git diff -- tracked.txt",
        "git diff-tree HEAD~1 HEAD",
        "git rev-list -p HEAD",
        "git format-patch HEAD~1",
        "git diff --no-ext-diff --no-textconv --textconv tracked.txt",
        "git diff --no-ext-diff --no-textconv --ext-diff tracked.txt",
        "git log -p",
        "git show HEAD",
        "git stash show",
        "git archive --format=tar HEAD",
        "git cat-file --filters HEAD:path",
        "git grep --textconv needle",
        "git blame --textconv HEAD -- file.txt",
        "git help status",
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
        "deno eval code",
        "deno repl --eval code",
        "deno serve server.ts",
        "deno bench bench.ts",
        "mix",
        "mix help",
        "mix --help",
        "mix test",
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
        "cmake .",
        "cmake --build .",
        "cmake -P project.cmake",
        "gradle build",
        "gradle -p app test",
        "ninja leak",
        "ninja -C build",
        "mvn test",
        "mvn -f app/pom.xml verify",
        "ctest",
        "ctest -N",
        "ctest --test-dir build",
        "pytest --help",
        "pytest -h",
        "pytest test_module --version",
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

    let words = [
        "GIT_CONFIG_COUNT=0",
        "GIT_CONFIG_KEY_0=core.fsmonitor",
        "GIT_CONFIG_VALUE_0=./leak",
        "git",
        "status",
    ]
    .map(str::to_string);
    let error = super::super::command_wrappers::validate_opaque_command_dispatchers(
        &[words.to_vec()],
        "test",
        true,
    )
    .expect_err("Git environment config must not bypass the fsmonitor override");
    assert!(
        error
            .to_string()
            .contains("Git config environment override"),
        "{error}"
    );

    for command in [
        "git status",
        "exec -l git status",
        "git -C . diff --no-ext-diff --no-textconv",
        "git diff-tree --no-ext-diff --no-textconv HEAD~1 HEAD",
        "git rev-list --no-ext-diff --no-textconv -p HEAD",
        "git format-patch --no-ext-diff --no-textconv HEAD~1",
        "git diff --no-ext-diff --no-textconv -- --ext-diff",
        "git --no-pager log --no-ext-diff --no-textconv -p",
        "git show --no-ext-diff --no-textconv HEAD",
        "git stash show --no-ext-diff --no-textconv",
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
        "mix --version",
        "git --help",
        "git -h",
        "cmake --help",
        "cmake --version",
        "gradle --help",
        "gradle --version",
        "gradle -v",
        "ninja --help",
        "ninja --version",
        "mvn --help",
        "mvn --version",
        "mvn -v",
        "ctest --help",
        "ctest -h",
        "ctest --version",
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
        "env -i git status",
        "env - git status",
        "env --ignore-environment git status",
        "env -u GIT_CONFIG_COUNT git status",
        "env -uGIT_CONFIG_COUNT git status",
        "env --unset GIT_CONFIG_COUNT git status",
        "env --unset=GIT_CONFIG_KEY_0 git status",
        "command env -i git status",
        "exec -c git status",
        "exec -cl git status",
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
            error
                .to_string()
                .contains("Git config environment override"),
            "{command}: {error}"
        );
    }

    let shell_command = super::super::shell_commands("env -i sh -c 'git status'").unwrap();
    let error = super::super::command_wrappers::validate_opaque_command_dispatchers(
        &shell_command,
        "test",
        true,
    )
    .expect_err("environment-clearing shell wrappers can remove Git config safeguards");
    assert!(
        error
            .to_string()
            .contains("Git config environment override"),
        "{error}"
    );

    let shell_command = super::super::shell_commands("env - sh -c 'git status'").unwrap();
    let error = super::super::command_wrappers::validate_opaque_command_dispatchers(
        &shell_command,
        "test",
        true,
    )
    .expect_err("bare env dash can clear Git config safeguards around shell-dispatched Git");
    assert!(
        error
            .to_string()
            .contains("Git config environment override"),
        "{error}"
    );

    let exec_shell_command = super::super::shell_commands("exec -c sh -c 'git status'").unwrap();
    let error = super::super::command_wrappers::validate_opaque_command_dispatchers(
        &exec_shell_command,
        "test",
        true,
    )
    .expect_err("exec -c can clear Git config safeguards around shell-dispatched Git");
    assert!(
        error
            .to_string()
            .contains("Git config environment override"),
        "{error}"
    );

    for command in ["env LANG=C git status", "env -u HOME git status"] {
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
