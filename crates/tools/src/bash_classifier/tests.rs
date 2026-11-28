use super::{classify_bash_command, is_idempotent_verification_command};

#[test]
fn test_classify_bash_command_priority_network_over_write() {
    let cap = classify_bash_command("mkdir out && curl https://example.com");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_path_qualified_network_tool() {
    // Path-qualified executables classify by basename so an approved network
    // call isn't run with the sandbox network deny still in force.
    assert_eq!(
        classify_bash_command("/usr/bin/curl example.com"),
        alan_agent_protocol::ToolCapability::Network
    );
    assert_eq!(
        classify_bash_command("/usr/bin/wget https://example.com/x"),
        alan_agent_protocol::ToolCapability::Network
    );
    // Path-qualified write tools likewise classify by basename.
    assert_eq!(
        classify_bash_command("/bin/rm file.txt"),
        alan_agent_protocol::ToolCapability::Write
    );
    // Path-qualified git subcommands classify via the basename gate too.
    assert_eq!(
        classify_bash_command("/usr/bin/git -C repo push"),
        alan_agent_protocol::ToolCapability::Network
    );
    assert_eq!(
        classify_bash_command("/usr/bin/git fetch origin"),
        alan_agent_protocol::ToolCapability::Network
    );
}

#[test]
fn test_classify_bash_command_write() {
    let cap = classify_bash_command("git reset --hard");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_read() {
    let cap = classify_bash_command("rg TODO src");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_treats_regex_pipe_inside_quotes_as_read() {
    let cap =
        classify_bash_command("rg -n \"resolve_redirects|303|307|308|redirect\" requests tests");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_treats_cd_then_read_as_read() {
    let cap = classify_bash_command("cd /tmp/repo && ls");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_treats_cd_then_network_as_network() {
    let cap = classify_bash_command("cd /tmp/repo && curl https://example.com");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_treats_cd_then_write_as_write() {
    let cap = classify_bash_command("cd /tmp/repo && rm -f artifact.txt");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_defaults_ambiguous_python_eval_to_unknown() {
    let cap = classify_bash_command("python -c \"print('hi')\"");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_python_script_file_as_unknown() {
    let cap = classify_bash_command("python3 script.py");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_shell_script_file_as_unknown() {
    let cap = classify_bash_command("bash script.sh");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_awk_script_file_as_unknown() {
    let cap = classify_bash_command("awk -f script.awk input.txt");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_shell_eval_wrappers_as_unknown() {
    let cap = classify_bash_command("bash -lc \"rg TODO src\"");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_shell_eval_wrappers_with_leading_options_as_unknown() {
    let cap = classify_bash_command("bash --noprofile -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_python_eval_wrappers_with_leading_options_as_unknown() {
    let cap = classify_bash_command("python3 -B -c 'print(\"hi\")'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_node_inline_long_eval_wrapper_as_unknown() {
    let cap = classify_bash_command("node --eval='require(\"fs\").writeFileSync(\"x\", \"y\")'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_shell_inline_long_command_wrapper_as_unknown() {
    let cap = classify_bash_command("sh --command='rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_eval_wrapper_with_line_continuation_as_unknown() {
    let cap = classify_bash_command("s\\\nh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_node_print_eval_wrappers_as_unknown() {
    let cap = classify_bash_command(
        "node --trace-warnings -p 'require(\"fs\").writeFileSync(\"x\", \"y\")'",
    );
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_allows_literal_sh_dash_c_arguments() {
    let cap = classify_bash_command("printf '%s %s' sh -c");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_treats_multiline_eval_wrapper_as_unknown() {
    let cap = classify_bash_command("echo ok\nsh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_post_comment_line_continuation_network_as_network() {
    let cap = classify_bash_command("echo ok #\\\ncurl https://example.com");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_treats_env_shell_eval_wrappers_as_unknown() {
    let cap = classify_bash_command("env FOO=bar sh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_bang_prefixed_shell_eval_as_unknown() {
    let cap = classify_bash_command("! sh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_then_prefixed_shell_eval_as_unknown() {
    let cap = classify_bash_command("if true; then sh -c 'rg TODO src'; fi");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_command_wrapper_shell_eval_as_unknown() {
    let cap = classify_bash_command("command -p sh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_nice_wrapper_as_unknown() {
    let cap = classify_bash_command("nice -n 5 sh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_time_wrapper_as_unknown() {
    let cap = classify_bash_command("time sh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_command_query_mode_as_unknown() {
    let cap = classify_bash_command("command -v sh -c");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_timeout_query_mode_as_unknown() {
    let cap = classify_bash_command("timeout --version");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_timeout_query_with_line_continuation_as_unknown() {
    let cap = classify_bash_command("time\\\nout --ver\\\nsion");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_builtin_query_mode_as_unknown() {
    let cap = classify_bash_command("builtin -p eval");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_exec_wrapper_shell_eval_with_argv0_as_unknown() {
    let cap = classify_bash_command("exec -a alan sh -c 'rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_stdbuf_wrapped_read_command_as_unknown() {
    let cap = classify_bash_command("stdbuf -oL rg TODO src");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_env_split_string_as_unknown() {
    let cap = classify_bash_command("env -S 'sh -c rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_clustered_env_split_string_as_unknown() {
    let cap = classify_bash_command("env -iS 'sh -c rg TODO src'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_treats_direct_command_with_leading_env_assignment_as_read() {
    let cap = classify_bash_command("ALAN_TEST=1 rg TODO src");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_redirection_without_whitespace_is_write() {
    let cap = classify_bash_command("echo x>.git/config");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_fetch_is_network() {
    let cap = classify_bash_command("git fetch origin main");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_git_fetch_with_global_options_is_network() {
    let cap = classify_bash_command("git -C /tmp/repo fetch --depth=1 origin main");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_git_rev_parse_with_global_options_is_read() {
    let cap = classify_bash_command("git -C /tmp/repo rev-parse --verify --quiet head");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_git_submodule_status_is_read() {
    let cap = classify_bash_command("git submodule status");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_git_submodule_init_is_write() {
    let cap = classify_bash_command("git submodule init");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_submodule_update_is_network() {
    let cap = classify_bash_command("git submodule update");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_git_submodule_update_no_fetch_is_write() {
    let cap = classify_bash_command("git submodule update --no-fetch");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_mutations_are_write() {
    let cap = classify_bash_command("git add .");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_branch_creation_is_write() {
    let cap = classify_bash_command("git branch release");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_branch_list_with_global_options_is_read() {
    let cap = classify_bash_command("git -C /tmp/repo branch --list");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_git_branch_edit_description_is_write() {
    let cap = classify_bash_command("git branch --edit-description");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_tag_creation_is_write() {
    let cap = classify_bash_command("git tag v1.2.3");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_tag_list_with_global_options_is_read() {
    let cap = classify_bash_command("git -C /tmp/repo tag --list");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_git_remote_add_is_write() {
    let cap =
        classify_bash_command("git remote add origin git@github.com:thinksmart-systems/alan.git");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_git_remote_add_fetch_is_network() {
    let cap = classify_bash_command("git remote add -f origin https://example.com/repo.git");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_git_remote_add_long_fetch_is_network() {
    let cap = classify_bash_command("git remote add --fetch origin https://example.com/repo.git");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_git_ls_remote_is_network() {
    let cap = classify_bash_command("git ls-remote origin");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_git_push_is_network() {
    let cap = classify_bash_command("git push origin main");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Network);
}

#[test]
fn test_classify_bash_command_sed_in_place_is_write() {
    let cap = classify_bash_command("sed -i 's/foo/bar/' src/lib.rs");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_sed_clustered_ei_is_write() {
    let cap = classify_bash_command("sed -Ei 's/foo/bar/' src/lib.rs");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_sed_clustered_ni_is_write() {
    let cap = classify_bash_command("sed -ni 's/foo/bar/' src/lib.rs");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_find_exec_is_write() {
    let cap = classify_bash_command("find . -name '*.tmp' -exec rm -f {} \\;");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_find_fprint_is_write() {
    let cap = classify_bash_command("find . -name '*.rs' -fprint /tmp/files.txt");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_find_fprint0_is_write() {
    let cap = classify_bash_command("find . -name '*.rs' -fprint0 /tmp/files.bin");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_find_name_defaults_to_read() {
    let cap = classify_bash_command("find . -name '*.rs'");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_find_pipeline_is_read() {
    let cap = classify_bash_command(
        "find . -maxdepth 3 \\( -path './test*' -o -path './tests*' \\) -type d | sort",
    );
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_pytest_is_write() {
    let cap = classify_bash_command("pytest tests/test_requests.py -k redirect");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_python_module_pytest_is_write() {
    let cap = classify_bash_command("python -B -m pytest tests/test_requests.py -k redirect");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_local_runtests_script_is_write() {
    let cap = classify_bash_command("./tests/runtests.py utils_tests.test_html");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_python_local_runtests_script_is_write() {
    let cap = classify_bash_command("python3 -B tests/runtests.py utils_tests.test_html");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_manage_py_test_is_write() {
    let cap = classify_bash_command("python manage.py test auth_tests");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_manage_py_shell_stays_unknown() {
    let cap = classify_bash_command("python manage.py shell");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_local_gradle_test_wrapper_is_write() {
    let cap = classify_bash_command("./gradlew test");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_tox_version_is_read() {
    let cap = classify_bash_command("tox --version");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_nox_help_is_read() {
    let cap = classify_bash_command("nox --help");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_tox_run_is_write() {
    let cap = classify_bash_command("tox -e py");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_nox_run_is_write() {
    let cap = classify_bash_command("nox -s tests");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_python_version_is_read() {
    let cap = classify_bash_command("python --version");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_sed_print_is_read() {
    let cap = classify_bash_command("sed -n '1,80p' test_requests.py");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_sed_substitute_is_read() {
    let cap = classify_bash_command("sed 's#^./##' test_requests.py");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_read_only_find_sed_pipeline_is_read() {
    let cap = classify_bash_command(
        "find . -maxdepth 2 -type f | sed 's#^./##' | sort | rg \"(^test|tests|requests/test)\"",
    );
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_sed_write_script_is_unknown() {
    let cap = classify_bash_command("sed -n '1,80w /tmp/out' test_requests.py");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Unknown);
}

#[test]
fn test_classify_bash_command_cargo_test_is_write() {
    let cap = classify_bash_command("cargo test -p alan-agent-engine delegated_skill --lib");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Write);
}

#[test]
fn test_classify_bash_command_openspec_read_subcommands_are_read() {
    use alan_agent_protocol::ToolCapability;
    for cmd in [
        "openspec validate --strict",
        "openspec list",
        "openspec show add-event-driven-plan",
        "openspec status",
        "openspec doctor",
        "openspec view",
        "openspec --help",
        "openspec -V",
        "openspec --version",
    ] {
        assert_eq!(
            classify_bash_command(cmd),
            ToolCapability::Read,
            "expected Read for `{cmd}`"
        );
    }
}

#[test]
fn test_classify_bash_command_openspec_validate_mixed_with_echo_is_read() {
    let cap = classify_bash_command("echo validating && openspec validate --strict");
    assert_eq!(cap, alan_agent_protocol::ToolCapability::Read);
}

#[test]
fn test_classify_bash_command_openspec_mutating_subcommands_are_not_read() {
    use alan_agent_protocol::ToolCapability;
    for cmd in [
        "openspec archive -y",
        "openspec init",
        "openspec update",
        "openspec install",
        "openspec new change",
        "openspec config list",
        "openspec store list",
        "openspec change archive",
    ] {
        let cap = classify_bash_command(cmd);
        assert_ne!(cap, ToolCapability::Read, "expected non-Read for `{cmd}`");
        assert_ne!(
            cap,
            ToolCapability::Network,
            "expected non-Network for `{cmd}`"
        );
    }
}

#[test]
fn test_classify_project_local_dev_build_format_and_git_commands() {
    use alan_agent_protocol::ToolCapability;
    for cmd in [
        "cargo build",
        "cargo fmt --check",
        "cargo +nightly fmt",
        "cargo run --bin alan",
        "cargo doc --no-deps",
        "pnpm check",
        "pnpm lint",
        "pnpm build",
        "pnpm dev",
        "npm run build",
        "npm run check",
        "yarn lint",
        "bun dev",
        "just",
        "just quality",
        "just fmt",
        "git add -A",
        "git commit -m 'wip'",
    ] {
        assert_eq!(classify_bash_command(cmd), ToolCapability::Write, "{cmd}");
    }
    for cmd in ["git status", "git diff", "git log -n 5"] {
        assert_eq!(classify_bash_command(cmd), ToolCapability::Read, "{cmd}");
    }
    for cmd in [
        "git push origin main",
        "cargo install ripgrep",
        "npm install",
    ] {
        assert_ne!(classify_bash_command(cmd), ToolCapability::Write, "{cmd}");
        assert_ne!(classify_bash_command(cmd), ToolCapability::Read, "{cmd}");
    }
}

#[test]
fn test_classify_bash_command_gh_network_and_local_queries() {
    use alan_agent_protocol::ToolCapability;
    for cmd in [
        "gh issue list",
        "gh pr view",
        "gh pr view 123",
        "/opt/homebrew/bin/gh api repos/thinksmart-systems/alan",
        "GH_TOKEN=tok /opt/homebrew/bin/gh issue list",
    ] {
        assert_eq!(classify_bash_command(cmd), ToolCapability::Network, "{cmd}");
    }

    for cmd in [
        "gh",
        "gh --version",
        "gh -v",
        "gh version",
        "gh help",
        "gh --help",
        "gh -h",
    ] {
        assert_ne!(classify_bash_command(cmd), ToolCapability::Network, "{cmd}");
    }
}

#[test]
fn test_classify_bash_command_distinguishes_standalone_env_from_wrapper() {
    use alan_agent_protocol::ToolCapability;

    for read_cmd in [
        "env",
        "/usr/bin/env",
        "env -0",
        "env --null",
        "FOO=bar env",
        "env | grep -E 'CARGO|RUST' || true",
        "env < /dev/null | grep -E 'CARGO|RUST' || true",
        "env | rg -i 'cargo|rust' || false",
        "printenv",
        "printenv PATH",
        "printenv | grep -E 'CARGO|RUST' || true",
    ] {
        assert_eq!(
            classify_bash_command(read_cmd),
            ToolCapability::Read,
            "expected Read for `{read_cmd}`"
        );
    }

    assert_eq!(
        classify_bash_command("env > env.txt"),
        ToolCapability::Write
    );

    for wrapper_cmd in [
        "env FOO=bar rg TODO src",
        "env -i cargo test",
        "env -- FOO=bar rg TODO src",
        "env -S 'sh -c rg TODO src'",
        "env FOO=bar sh -c 'rg TODO src'",
        "env -u PATH cargo test",
        "env -u CARGO_HOME cargo test",
        "env -u RUSTUP_HOME cargo test",
        "env -u LD_PRELOAD cargo test",
        "env -u DYLD_INSERT_LIBRARIES cargo test",
        "env -iu BASH_ENV cargo test",
        "env -u BASH_ENV FOO=bar cargo test",
        "env -u BASH_ENV env -i cargo test",
        "env -u BASH_ENV sh -c 'rm -rf .'",
    ] {
        assert_eq!(
            classify_bash_command(wrapper_cmd),
            ToolCapability::Unknown,
            "expected Unknown for wrapper `{wrapper_cmd}`"
        );
    }

    for read_unset_cmd in [
        "env -u BASH_ENV rg TODO src",
        "env --unset=BASH_ENV cat README.md",
        "env -uBASH_ENV ls -la",
        "env -u BASH_ENV -- which cargo",
    ] {
        assert_eq!(
            classify_bash_command(read_unset_cmd),
            ToolCapability::Read,
            "expected Read for safe unset wrapper `{read_unset_cmd}`"
        );
    }

    for write_unset_cmd in [
        "env -u BASH_ENV cargo test -p alan-shell",
        "env --unset=BASH_ENV cargo clippy --all-targets",
        "env -uBASH_ENV cargo fmt --all -- --check",
        "env -u BASH_ENV -- cargo check --workspace",
    ] {
        assert_eq!(
            classify_bash_command(write_unset_cmd),
            ToolCapability::Write,
            "expected Write for safe unset wrapper `{write_unset_cmd}`"
        );
    }

    for network_unset_cmd in [
        "env -u BASH_ENV curl https://example.com",
        "env -u BASH_ENV cargo install ripgrep",
    ] {
        assert_eq!(
            classify_bash_command(network_unset_cmd),
            ToolCapability::Network,
            "expected Network for safe unset wrapper `{network_unset_cmd}`"
        );
    }
}

#[test]
fn test_is_idempotent_verification_command_allows_local_checks() {
    for command in [
        "cargo check --workspace",
        "cargo clippy --all-targets --all-features -- -D warnings",
        "cargo test -p alan-tools --all-targets",
        "cargo doc --no-deps",
        "cargo fmt --all -- --check",
        "go test ./...",
        "pytest -q",
        "py.test tests",
        "nosetests3",
        "npm test",
        "pnpm run lint",
        "yarn run typecheck",
        "npm run format:check",
        "just check",
        "just quality",
        "just lint",
        "just test",
        "just --list",
        "env -u BASH_ENV cargo fmt --all -- --check",
        "cargo check && cargo test",
        "rg TODO src",
        "git status",
    ] {
        assert!(
            is_idempotent_verification_command(command),
            "expected idempotent verification for `{command}`"
        );
    }
}

#[test]
fn test_is_idempotent_verification_command_rejects_effectful_and_opaque_forms() {
    for command in [
        "cargo fmt",
        "cargo fmt --all",
        "cargo clippy --fix",
        "cargo build",
        "cargo run",
        "cargo install ripgrep",
        "cargo test -- --watch",
        "just fmt",
        "just verify",
        "just",
        "npm run build",
        "pnpm add left-pad",
        "npm install",
        "yarn dev",
        "rm -rf target",
        "write_file output.txt",
        "curl https://example.com",
        "git commit -m change",
        "cargo test > out.txt",
        "cargo test 2>&1 | tee out.txt",
        "python3 script.py",
        "bash -lc 'cargo test'",
        "echo ok && rm -f artifact.txt",
        "",
        "   ",
    ] {
        assert!(
            !is_idempotent_verification_command(command),
            "expected non-idempotent classification for `{command}`"
        );
    }
}
