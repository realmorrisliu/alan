//! Admission of native sequences whose inspected Rust lookup inputs remain unchanged.
use super::*;
use crate::tools::sandbox::command_and_args;
use crate::tools::sandbox::command_wrappers::{
    command_wrapper_executables, is_env_assignment, shell_wrapper_inline_script,
};

const SEPARATE: &str = "Inspected Rust commands need separate requests after other programs or Rust execution; use literal cd/set, then run each build/test/application separately";

fn rust_command(words: &[String]) -> Option<(&str, &[String])> {
    let (name, args) = command_and_args(words)?;
    PROXIES.contains(&name).then_some((name, args))
}

fn query(name: &str, args: &[String]) -> bool {
    if name == "cargo" {
        return crate::tools::sandbox::command_options::cargo_global_query(args);
    }
    let args = if args.first().is_some_and(|arg| arg.starts_with('+')) {
        &args[1..]
    } else {
        args
    };
    matches!(args, [arg] if matches!(arg.as_str(), "--version" | "-V" | "-vV" | "--help" | "-h"))
        && matches!(name, "rustup" | "rustc" | "rustdoc" | "rustfmt")
}

fn projected_provider(
    executable: &str,
    path: &str,
    input: &ReifiedNamespacePlanInput,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<ReifiedRustCommandProvider, String> {
    let plan = super::super::ReifiedNamespacePlan::derive(input.clone())
        .map_err(|error| error.to_string())?;
    let mut mounts = plan
        .declared_host_mounts
        .iter()
        .map(|mount| ReifiedExecutionSubstrateMount::new(&mount.namespace_path, &mount.host_path))
        .chain(plan.execution_substrate.iter().cloned())
        .chain(proxies.iter().cloned())
        .collect::<Vec<_>>();
    mounts.sort_by_key(|mount| std::cmp::Reverse(mount.namespace_path.components().count()));
    let source = provider::resolve_projected(executable, path, &mounts)?;
    let resolved = source
        .map(|source| {
            let proxy = provider::is_proxy(Some(&source), proxies)?;
            executable_digest(&source).map(|digest| (source, digest, proxy))
        })
        .transpose()?;
    Ok(ReifiedRustCommandProvider {
        executable: resolved.as_ref().map_or_else(
            || executable.to_owned(),
            |(source, _, _)| source.to_string_lossy().into_owned(),
        ),
        path: String::new(),
        resolved,
        requires_private_proxy_links: false,
    })
}

fn standard_source(executable: &str, source: &Path) -> bool {
    Path::new(executable).file_name().is_some_and(|name| {
        [
            PathBuf::from("/usr/bin").join(name),
            PathBuf::from("/bin").join(name),
        ]
        .iter()
        .any(|standard| std::fs::canonicalize(standard).is_ok_and(|path| path == source))
    })
}

fn standard_provider(
    executable: &str,
    path: &str,
    input: &ReifiedNamespacePlanInput,
) -> Result<ReifiedRustCommandProvider, String> {
    let binding = projected_provider(executable, path, input, &[])?;
    if !binding
        .resolved
        .as_ref()
        .is_some_and(|(source, _, _)| standard_source(executable, source))
    {
        return Err(SEPARATE.into());
    }
    Ok(binding)
}

pub(super) fn proxy_links_stay_in_directory(source: &Path) -> bool {
    let Ok(identity) = std::fs::metadata(source) else {
        return false;
    };
    if identity.nlink() == 1 {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(source.parent().unwrap()) else {
        return false;
    };
    let mut links = 0;
    for entry in entries.take(PROXIES.len() + 1) {
        let Ok(metadata) = entry.and_then(|entry| std::fs::symlink_metadata(entry.path())) else {
            return false;
        };
        if metadata.is_file()
            && metadata.dev() == identity.dev()
            && metadata.ino() == identity.ino()
        {
            links += 1;
        }
    }
    // Canonical paths do not reveal writable hard links outside the validated proxy directory.
    links == identity.nlink()
}

fn query_provider(
    executable: &str,
    path: &str,
    input: &ReifiedNamespacePlanInput,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<Option<ReifiedRustCommandProvider>, String> {
    let mut binding = projected_provider(executable, path, input, proxies)?;
    // A frozen file identity does not prove that a project program implements a read-only query.
    let trusted = binding.resolved.as_ref().is_some_and(|(source, _, proxy)| {
        standard_source(executable, source)
            || *proxy
                && proxy_links_stay_in_directory(source)
                && !input.declarations.iter().any(|mount| {
                    matches!(&mount.source, super::super::ReifiedMountSource::Host(root)
                        if super::super::plan::paths_overlap(&canonicalize_existing_host_path(root), source.parent().unwrap()))
                })
    });
    binding.requires_private_proxy_links = trusted
        && binding
            .resolved
            .as_ref()
            .is_some_and(|(source, _, proxy)| *proxy && !standard_source(executable, source));
    Ok(trusted.then_some(binding))
}

fn wrappers(
    words: &[String],
    path: &str,
    input: &ReifiedNamespacePlanInput,
) -> Result<Vec<ReifiedRustCommandProvider>, String> {
    command_wrapper_executables(words)
        .into_iter()
        .filter_map(|executable| {
            if matches!(executable, "command" | "exec" | "builtin") {
                return None;
            }
            let name = Path::new(executable)
                .file_name()
                .and_then(|name| name.to_str());
            Some(
                if matches!(name, Some("env" | "nice" | "nohup" | "timeout" | "stdbuf")) {
                    standard_provider(executable, path, input)
                } else {
                    Err(SEPARATE.into())
                },
            )
        })
        .collect()
}

pub(super) fn inspect(
    input: &ReifiedNamespacePlanInput,
    path: &str,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<Vec<ReifiedRustCommandProvider>, String> {
    let Some(script) =
        shell_wrapper_inline_script(&input.argv).map_err(|error| error.to_string())?
    else {
        return if rust_command(&input.argv).is_some() {
            wrappers(&input.argv, path, input)
        } else {
            Ok(Vec::new())
        };
    };
    inspect_script(&script, path, input, proxies)
}

fn inspect_script(
    script: &str,
    path: &str,
    input: &ReifiedNamespacePlanInput,
    proxies: &[ReifiedExecutionSubstrateMount],
) -> Result<Vec<ReifiedRustCommandProvider>, String> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_bash::LANGUAGE.into())
        .map_err(|error| error.to_string())?;
    let tree = parser.parse(script, None).ok_or(SEPARATE)?;
    let mut nodes = Vec::new();
    let mut commands = Vec::new();
    let mut pending = vec![tree.root_node()];
    while let Some(node) = pending.pop() {
        if node.kind() == "command" {
            let words = crate::tools::sandbox::shell_commands(&script[node.byte_range()])
                .map_err(|error| error.to_string())?;
            commands.extend(words.into_iter().map(|words| (node, words)));
        }
        nodes.push(node);
        pending.extend(node.children(&mut node.walk()));
    }
    commands.sort_by_key(|(node, _)| node.start_byte());
    let Some(last) = commands
        .iter()
        .rev()
        .find(|(_, words)| rust_command(words).is_some())
        .map(|(node, _)| *node)
    else {
        return Ok(Vec::new());
    };
    if tree.root_node().has_error()
        || nodes.iter().any(|node| {
            matches!(
                node.kind(),
                "&" | "command_substitution" | "process_substitution"
            ) || node.is_named()
                && node.start_byte() < last.end_byte()
                && !matches!(
                    node.kind(),
                    "program"
                        | "list"
                        | "command"
                        | "command_name"
                        | "word"
                        | "raw_string"
                        | "string"
                        | "string_content"
                        | "concatenation"
                        | "comment"
                        | "redirected_statement"
                        | "heredoc_redirect"
                        | "heredoc_body"
                        | "heredoc_start"
                        | "heredoc_end"
                        | "variable_assignment"
                        | "variable_name"
                        | "number"
                        | "pipeline"
                        | "subshell"
                        | "compound_statement"
                )
                || node.kind() == "variable_assignment"
                    && node.start_byte() < last.end_byte()
                    && (node.parent().is_none_or(|parent| {
                        !commands.iter().any(|(command, words)| {
                            *command == parent && rust_command(words).is_some()
                        })
                    }) || !is_env_assignment(&script[node.byte_range()]))
        })
    {
        return Err(SEPARATE.into());
    }
    let mut changed = false;
    let mut providers = Vec::new();
    for (node, words) in commands
        .iter()
        .take_while(|(node, _)| node.start_byte() <= last.start_byte())
    {
        if let Some((name, args)) = rust_command(words) {
            if changed {
                return Err(SEPARATE.into());
            }
            let mut ancestor = node.parent();
            while let Some(parent) = ancestor {
                if !matches!(parent.kind(), "program" | "list")
                    && !(parent.kind() == "pipeline"
                        && name == "rustc"
                        && parent.end_byte() == node.end_byte())
                {
                    return Err(SEPARATE.into());
                }
                ancestor = parent.parent();
            }
            providers.extend(wrappers(words, path, input)?);
            changed = true;
            if query(name, args) && *node != last {
                let executable = &words[words.len() - args.len() - 1];
                if let Some(binding) = query_provider(executable, path, input, proxies)? {
                    providers.push(binding);
                    changed = false;
                }
            }
        } else if command_and_args(words)
            .is_some_and(|(name, args)| name == "git" && args == ["--version"])
        {
            providers.extend(wrappers(words, path, input)?);
            let (_, args) = command_and_args(words).ok_or(SEPARATE)?;
            providers.push(standard_provider(
                &words[words.len() - args.len() - 1],
                path,
                input,
            )?);
        } else if words.first().is_some_and(|word| word == "printf")
            && words.get(1).is_some_and(|format| {
                !format.starts_with('-')
                    && (!format.contains('%') || matches!(format.as_str(), "%s" | "%s\\n"))
            })
        {
        } else if words.first().is_some_and(|word| word == "cat") {
            providers.push(standard_provider("cat", path, input)?);
        } else {
            let mut ancestor = node.parent();
            let mut nested = false;
            while let Some(parent) = ancestor {
                nested |= !matches!(parent.kind(), "program" | "list");
                ancestor = parent.parent();
            }
            changed |= nested
                || (!words.first().is_some_and(|word| word == "cd")
                    && !(words.first().is_some_and(|word| word == "set")
                        && words[1..]
                            .iter()
                            .all(|arg| matches!(arg.as_str(), "-e" | "-u" | "-eu" | "-ue"))));
        }
    }
    Ok(providers)
}

#[cfg(test)]
#[path = "rustup_sequence_tests.rs"]
mod tests;
