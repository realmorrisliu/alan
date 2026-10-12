//! Bounded Cargo compiler/configuration admission; never resolves custom compilers.
use super::super::plan::ReifiedCargoConfigInput;
use super::*;

pub(super) use crate::tools::sandbox::command_wrappers::rust_compiler_override as compiler_override;

pub(super) fn uses_compiler(args: &[String]) -> bool {
    use crate::tools::sandbox::command_options::{cargo_global_query, cargo_subcommand_index};
    if cargo_global_query(args) {
        return false;
    }
    let Some(index) = cargo_subcommand_index(args) else {
        return false;
    };
    let command = args[index].as_str();
    if matches!(
        command,
        "locate-project" | "new" | "init" | "version" | "read-manifest"
    ) {
        return false;
    }
    if command == "help" {
        // Cargo dispatches named help targets through aliases or external commands.
        return help_target(&args[index + 1..])
            .is_some_and(|target| !matches!(target, "fmt" | "clippy"));
    }
    let builtin = is_builtin(command);
    let mut visible = args[index + 1..]
        .iter()
        .take_while(|arg| arg.as_str() != "--");
    while let Some(arg) = visible.next() {
        if matches!(
            arg.as_str(),
            "--config"
                | "--color"
                | "-Z"
                | "-C"
                | "--manifest-path"
                | "--format-version"
                | "--filter-platform"
                | "--features"
                | "-F"
        ) {
            visible.next();
        } else if (builtin || matches!(command, "fmt" | "clippy"))
            && matches!(arg.as_str(), "--help" | "-h")
            || command == "clippy" && matches!(arg.as_str(), "--version" | "-V")
            || command == "metadata" && arg == "--no-deps"
        {
            return false;
        }
    }
    true
}

fn is_builtin(command: &str) -> bool {
    matches!(
        command,
        "locate-project"
            | "new"
            | "init"
            | "help"
            | "version"
            | "read-manifest"
            | "build"
            | "check"
            | "test"
            | "run"
            | "bench"
            | "doc"
            | "rustc"
            | "rustdoc"
            | "clean"
            | "metadata"
            | "fetch"
            | "tree"
            | "generate-lockfile"
            | "update"
            | "package"
            | "publish"
            | "install"
            | "uninstall"
            | "search"
            | "info"
            | "login"
            | "logout"
            | "owner"
            | "yank"
            | "add"
            | "remove"
            | "vendor"
            | "fix"
            | "report"
    )
}

// shortcut: fresh private Cargo home is empty; freeze its bin before supporting in-request helper edits.
pub(super) fn implicit_tools(args: &[String]) -> Vec<String> {
    if crate::tools::sandbox::command_options::cargo_global_query(args) {
        return Vec::new();
    }
    let Some(command) = cargo_subcommand(args) else {
        return Vec::new();
    };
    let command = if command == "help" {
        let index = crate::tools::sandbox::command_options::cargo_subcommand_index(args).unwrap();
        let target = help_target(&args[index + 1..]).unwrap_or(command);
        if is_builtin(target) {
            return Vec::new();
        }
        target
    } else {
        command
    };
    if !uses_compiler(args) {
        return if matches!(command, "fmt" | "clippy") {
            vec![format!("cargo-{command}")]
        } else {
            Vec::new()
        };
    }
    match command {
        "doc" | "d" | "rustdoc" => vec!["rustc".into(), "rustdoc".into()],
        "test" | "t"
            if !args
                .iter()
                .take_while(|arg| arg.as_str() != "--")
                .any(|arg| {
                    matches!(
                        arg.as_str(),
                        "--lib"
                            | "--bins"
                            | "--bin"
                            | "--tests"
                            | "--test"
                            | "--benches"
                            | "--bench"
                            | "--examples"
                            | "--example"
                            | "--all-targets"
                            | "--no-run"
                    ) || ["--bin=", "--test=", "--bench=", "--example="]
                        .iter()
                        .any(|prefix| arg.starts_with(prefix))
                }) =>
        {
            vec!["rustc".into(), "rustdoc".into()]
        }
        "fmt" => vec!["cargo-fmt".into(), "rustfmt".into()],
        "clippy" => vec![
            "cargo-clippy".into(),
            "clippy-driver".into(),
            "rustc".into(),
        ],
        name if is_builtin(name) || matches!(name, "b" | "c" | "t" | "r" | "rm") => {
            vec!["rustc".into()]
        }
        name => vec![format!("cargo-{name}"), "rustc".into()],
    }
}

fn help_target(args: &[String]) -> Option<&str> {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        if arg == "--" {
            return args.next().map(String::as_str);
        }
        if matches!(arg.as_str(), "--config" | "--color" | "-C" | "-Z") {
            args.next()?;
        } else if !arg.starts_with('-') {
            return Some(arg);
        }
    }
    None
}

fn validate_config(bytes: &str, args: &[String]) -> Result<(), String> {
    let value: toml::Value = toml::from_str(bytes)
        .map_err(|error| format!("invalid inspected Cargo configuration: {error}"))?;
    if value.get("include").is_some()
        || value.get("build").is_some_and(|build| {
            [
                "rustc",
                "rustc-wrapper",
                "rustc-workspace-wrapper",
                "rustdoc",
            ]
            .iter()
            .any(|key| build.get(key).is_some())
        })
        || value
            .get("env")
            .and_then(toml::Value::as_table)
            .is_some_and(|env| {
                env.keys().any(|name| {
                    compiler_override(name)
                        || matches!(
                            name.as_str(),
                            "PATH" | "RUSTUP_HOME" | "RUSTUP_TOOLCHAIN" | "CARGO_HOME"
                        )
                })
            })
        || cargo_subcommand(args).is_some_and(|command| {
            let target = if command == "help" {
                let index = crate::tools::sandbox::command_options::cargo_subcommand_index(args)
                    .expect("a selected Cargo subcommand has an index");
                help_target(&args[index + 1..])
            } else {
                Some(command)
            };
            value.get("alias").is_some_and(|aliases| {
                aliases.get(command).is_some()
                    || target.is_some_and(|target| aliases.get(target).is_some())
            })
        })
    {
        return Err("Cargo compiler/wrapper overrides, config includes and selected aliases are unsupported".into());
    }
    Ok(())
}

fn read_config(path: &Path) -> Result<Option<String>, String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("Cargo configuration unavailable: {error}")),
        Ok(_) => read_metadata(path).map(Some),
    }
}

pub(super) fn revalidate(input: &ReifiedCargoConfigInput) -> Result<(), String> {
    if std::fs::canonicalize(&input.root).map_err(|error| error.to_string())? != input.root {
        return Err("Cargo configuration backing root changed before execution".into());
    }
    super::super::toolchain::validate_directory_aliases(
        &input.path,
        &input.root,
        &input.namespace_root,
    )?;
    if read_config(&input.path)?.as_deref().map(digest) != input.digest {
        return Err("Cargo configuration changed before execution".into());
    }
    Ok(())
}

fn inspect_file(
    plan: &super::super::ReifiedNamespacePlan,
    candidate: &Path,
    args: &[String],
    snapshots: &mut Vec<ReifiedCargoConfigInput>,
) -> Result<bool, String> {
    let projection = plan
        .declared_host_mounts
        .iter()
        .map(|m| (&m.namespace_path, &m.host_path))
        .chain(
            plan.execution_substrate
                .iter()
                .map(|m| (&m.namespace_path, &m.host_path)),
        )
        .filter(|(namespace, _)| candidate.starts_with(namespace))
        .max_by_key(|(namespace, _)| namespace.components().count());
    let Some((namespace, root)) = projection else {
        return Ok(false);
    };
    let source = root.join(candidate.strip_prefix(namespace).unwrap());
    super::super::toolchain::validate_directory_aliases(&source, root, namespace)?;
    let bytes = read_config(&source)?;
    if let Some(bytes) = &bytes {
        validate_config(bytes, args)?;
    }
    let hash = bytes.as_deref().map(digest);
    let entry = ReifiedCargoConfigInput {
        path: source,
        root: root.clone(),
        namespace_root: namespace.clone(),
        digest: hash.clone(),
    };
    if !snapshots.contains(&entry) {
        snapshots.push(entry);
    }
    Ok(hash.is_some())
}

pub(super) fn inspect(
    input: &ReifiedNamespacePlanInput,
    commands: &[Vec<String>],
) -> Result<Vec<ReifiedCargoConfigInput>, String> {
    let mut directories = Some(vec![canonicalize_existing_host_path(&input.cwd)]);
    let mut snapshots = Vec::new();
    let mut inspected_directories = Vec::new();
    let mut native_config_files = Vec::new();
    for words in commands {
        let Some((name, args)) = crate::tools::sandbox::command_and_args(words) else {
            continue;
        };
        if name == "cd" {
            directories = directories.and_then(|dirs| cd_directories(input, &dirs, args));
        }
        let (frontend, args) = if name == "rustup" && args.first().is_some_and(|arg| arg == "run") {
            let Some(frontend) = args.get(2) else {
                continue;
            };
            (frontend.as_str(), &args[3..])
        } else {
            (name, args)
        };
        let frontend_args;
        let args = match frontend {
            "cargo" => args,
            "cargo-clippy" | "cargo-fmt" => {
                frontend_args = std::iter::once(frontend.trim_start_matches("cargo-").to_string())
                    .chain(args.iter().cloned())
                    .collect::<Vec<_>>();
                &frontend_args
            }
            _ => continue,
        };
        if crate::tools::sandbox::command_options::cargo_global_help(args) {
            continue;
        }
        // shortcut: default aliases can discard query flags; use canonical commands until qualified.
        if crate::tools::sandbox::command_options::cargo_global_query(args)
            && !crate::tools::sandbox::command_options::cargo_global_help(args)
            && matches!(
                cargo_subcommand(args),
                Some("b" | "c" | "d" | "t" | "r" | "rm")
            )
        {
            return Err(
                "global Cargo version/list may execute default aliases; use a canonical subcommand"
                    .into(),
            );
        }
        // Global version/list flags can be discarded during configured alias expansion.
        if !uses_compiler(args)
            && implicit_tools(args).is_empty()
            && cargo_subcommand(args).is_none_or(is_builtin)
        {
            continue;
        }
        if crate::tools::sandbox::command_options::cargo_changes_directory(args) {
            return Err("Cargo -C changes the inspected compiler/configuration cwd".into());
        }
        for directory in directories
            .as_ref()
            .ok_or("Cargo compiler inspection needs a fixed delegated cwd")?
        {
            let mut inspection = input.clone();
            inspection.cwd = directory.clone();
            let plan = super::super::ReifiedNamespacePlan::derive(inspection)
                .map_err(|error| format!("Cargo configuration projection failed: {error}"))?;
            let mut native_directories = vec![plan.cwd.clone()];
            for mount in &plan.declared_host_mounts {
                if let Ok(suffix) = directory.strip_prefix(&mount.host_path) {
                    let target = mount.namespace_path.join(suffix);
                    if !native_directories.contains(&target) {
                        native_directories.push(target);
                    }
                }
            }
            for native_directory in native_directories {
                if !inspected_directories.contains(&native_directory) {
                    inspected_directories.push(native_directory.clone());
                }
                for ancestor in native_directory.ancestors() {
                    let legacy = ancestor.join(".cargo/config");
                    for path in [&legacy, &ancestor.join(".cargo/config.toml")] {
                        if !native_config_files.contains(path) {
                            native_config_files.push(path.clone());
                        }
                    }
                    if !inspect_file(&plan, &legacy, args, &mut snapshots)? {
                        inspect_file(
                            &plan,
                            &ancestor.join(".cargo/config.toml"),
                            args,
                            &mut snapshots,
                        )?;
                    }
                }
            }
            let mut visible = args.iter().take_while(|arg| arg.as_str() != "--");
            while let Some(word) = visible.next() {
                let value = if word == "--config" {
                    Some(visible.next().ok_or("missing Cargo config value")?.as_str())
                } else {
                    word.strip_prefix("--config=")
                };
                let Some(value) = value else { continue };
                if value.contains('=') {
                    validate_config(value, args)?;
                } else {
                    let file = Path::new(value);
                    if contains_parent_component(file) {
                        return Err("parent-traversing Cargo config is unsupported".into());
                    }
                    let file = plan.cwd.join(file);
                    if !inspect_file(&plan, &file, args, &mut snapshots)? {
                        return Err(
                            "Cargo config file is absent or outside delegated native authority"
                                .into(),
                        );
                    }
                }
            }
        }
    }
    // shortcut: opaque program-internal config mutation within one shell request is unqualified.
    if !snapshots.is_empty() {
        use crate::tools::sandbox::command_wrappers::{
            shell_wrapper_inline_script, writes_cargo_configuration,
        };
        if let Some(script) =
            shell_wrapper_inline_script(&input.argv).map_err(|error| error.to_string())?
        {
            let mut files = snapshots
                .iter()
                .map(|input| {
                    input.namespace_root.join(
                        input
                            .path
                            .strip_prefix(&input.root)
                            .expect("inspected root contains config"),
                    )
                })
                .collect::<Vec<_>>();
            files.extend(native_config_files);
            if writes_cargo_configuration(&script, &files, &inspected_directories)
                .map_err(|error| error.to_string())?
            {
                return Err(
                    "Cargo configuration changes and execution need separate requests".into(),
                );
            }
        }
    }
    Ok(snapshots)
}
