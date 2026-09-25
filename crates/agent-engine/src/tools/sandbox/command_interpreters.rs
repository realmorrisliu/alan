use super::command_options::{exact_or_inline_option_with_value, has_attached_option_value};
use super::command_project_dispatchers::is_project_code_dispatcher;

pub(super) fn opaque_command_dispatcher_display(
    display: &str,
    command: &str,
    args: &[String],
    reject_project_code_dispatchers: bool,
) -> Option<String> {
    if command == "xargs" {
        return Some(display.to_string());
    }
    if reject_project_code_dispatchers && command == "git" && has_uninspectable_git_dispatch(args) {
        return Some(format!("{display} extension"));
    }
    if reject_project_code_dispatchers && is_project_code_dispatcher(command, args) {
        return Some(display.to_string());
    }
    (command == "find")
        .then_some(())
        .and_then(|()| find_dispatch_clause(args))
        .map(|clause| format!("{display} {clause}"))
}

fn has_uninspectable_git_dispatch(args: &[String]) -> bool {
    // ponytail: only common built-ins are allowed; add names only for a real shell workflow need.
    const BUILTINS: &str = concat!(
        "add archive blame branch cat-file checkout cherry-pick clean clone commit ",
        "describe diff diff-files diff-index diff-tree fetch format-patch grep help init log ",
        "ls-files ls-tree merge merge-base mv notes pull push rebase remote reset restore ",
        "revert rev-list rev-parse rm shortlog show show-ref stash status switch tag worktree"
    );

    let mut index = 0;
    while let Some(argument) = args.get(index).map(String::as_str) {
        match argument {
            "-C" | "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix"
            | "--config-env" => {
                index += 2;
                if index > args.len() {
                    return true;
                }
            }
            "-p"
            | "--paginate"
            | "--no-pager"
            | "--bare"
            | "--no-replace-objects"
            | "--no-optional-locks"
            | "--literal-pathspecs"
            | "--glob-pathspecs"
            | "--noglob-pathspecs"
            | "--icase-pathspecs"
            | "--exec-path" => index += 1,
            "--version" | "-v" | "--help" | "-h" => return false,
            argument
                if argument.starts_with("-C")
                    || argument.starts_with("-c")
                    || argument.starts_with("--git-dir=")
                    || argument.starts_with("--work-tree=")
                    || argument.starts_with("--namespace=")
                    || argument.starts_with("--super-prefix=")
                    || argument.starts_with("--config-env=")
                    || argument.starts_with("--exec-path=") =>
            {
                index += 1;
            }
            argument if argument.starts_with('-') => return true,
            command => {
                let known_builtin = BUILTINS
                    .split_ascii_whitespace()
                    .any(|builtin| builtin == command);
                return !known_builtin
                    || git_subcommand_can_invoke_hooks(command, &args[index + 1..]);
            }
        }
    }
    false
}

fn git_subcommand_can_invoke_hooks(command: &str, args: &[String]) -> bool {
    const HOOK_DISPATCHERS: &str = concat!(
        "add am checkout cherry-pick clone commit fetch gc merge mv pull push rebase reset ",
        "restore revert rm switch"
    );

    if HOOK_DISPATCHERS
        .split_ascii_whitespace()
        .any(|dispatcher| dispatcher == command)
    {
        return true;
    }

    match command {
        "branch" => {
            !args.is_empty()
                && !args.iter().any(|arg| {
                    matches!(
                        arg.as_str(),
                        "-a" | "--all"
                            | "-l"
                            | "--list"
                            | "-r"
                            | "--remotes"
                            | "--show-current"
                            | "--contains"
                            | "--merged"
                            | "--no-merged"
                    )
                })
        }
        "tag" => {
            !args.is_empty()
                && !args
                    .iter()
                    .any(|arg| matches!(arg.as_str(), "-l" | "--list" | "--contains" | "--merged"))
        }
        "notes" => args.first().is_some_and(|subcommand| {
            matches!(
                subcommand.as_str(),
                "add" | "append" | "copy" | "edit" | "merge" | "prune" | "remove"
            )
        }),
        "stash" => !matches!(
            args.first().map(String::as_str),
            Some("list" | "show" | "help" | "--help")
        ),
        "worktree" => args.first().is_some_and(|subcommand| {
            matches!(subcommand.as_str(), "add" | "move" | "remove" | "prune")
        }),
        "remote" => args.first().is_some_and(|subcommand| {
            matches!(
                subcommand.as_str(),
                "update" | "prune" | "set-head" | "remove" | "rename"
            )
        }),
        _ => false,
    }
}

fn find_dispatch_clause(args: &[String]) -> Option<&'static str> {
    const FIND_DISPATCH_FLAGS: [&str; 4] = ["-exec", "-execdir", "-ok", "-okdir"];

    args.iter().enumerate().find_map(|(index, arg)| {
        let flag = FIND_DISPATCH_FLAGS
            .iter()
            .copied()
            .find(|flag| *flag == arg)?;
        let tail = &args[index + 1..];
        (!tail.first()?.starts_with('-')
            && tail
                .iter()
                .any(|candidate| candidate == ";" || candidate == "+"))
        .then_some(flag)
    })
}

pub(super) fn opaque_script_interpreter_display(
    display: &str,
    command: &str,
    args: &[String],
) -> Option<String> {
    match command {
        "sh" | "bash" | "dash" | "zsh" | "ksh" => shell_script_interpreter_display(display, args),
        "python" | "python3" => python_script_interpreter_display(display, args),
        "node" => node_script_interpreter_display(display, args),
        "perl" => perl_script_interpreter_display(display, args),
        "ruby" => ruby_script_interpreter_display(display, args),
        "lua" => lua_script_interpreter_display(display, args),
        "php" => php_script_interpreter_display(display, args),
        "awk" | "gawk" | "mawk" | "nawk" => awk_script_interpreter_display(display, args),
        _ => None,
    }
}

fn shell_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if shell_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-s" {
            return Some(format!("{display} -s"));
        }
        if let Some(step) = shell_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn python_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if python_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if matches!(arg, "-m" | "--module") {
            if args
                .get(index + 1)
                .is_some_and(|module| is_safe_python_module_runner(module))
            {
                return None;
            }
            return Some(format!("{display} {arg}"));
        }
        if arg == "-" {
            return Some(format!("{display} {arg}"));
        }
        if let Some(step) = python_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn is_safe_python_module_runner(module: &str) -> bool {
    matches!(module, "pytest" | "unittest")
}

fn node_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if node_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = node_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn perl_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if perl_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = perl_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn ruby_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if ruby_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = ruby_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn lua_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if lua_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if arg == "-" {
            return Some(format!("{display} -"));
        }
        if let Some(step) = lua_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn php_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if php_query_flag(arg) {
            return None;
        }
        if arg == "--" {
            return args
                .get(index + 1)
                .map(|script| format!("{display} {}", script));
        }
        if matches!(arg, "-B" | "-E" | "-R" | "-F" | "-") {
            return Some(format!("{display} {arg}"));
        }
        if exact_or_inline_option_with_value(arg, &["-f"], &["--file"]) {
            return Some(format!("{display} -f"));
        }
        if let Some(step) = php_wrapper_advance(arg) {
            index += step;
            continue;
        }
        return Some(format!("{display} {arg}"));
    }
    Some(format!("{display} <stdin>"))
}

fn awk_script_interpreter_display(display: &str, args: &[String]) -> Option<String> {
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if awk_query_flag(arg) {
            return None;
        }
        if arg == "-W" {
            if matches!(
                args.get(index + 1).map(|value| value.as_str()),
                Some("version" | "help")
            ) {
                return None;
            }
            index += 2;
            continue;
        }
        if arg == "--" {
            return args.get(index + 1).map(|_| format!("{display} program"));
        }
        if exact_or_inline_option_with_value(arg, &["-f"], &["--file"]) {
            return Some(format!("{display} -f"));
        }
        if exact_or_inline_option_with_value(arg, &["-i"], &["--include"]) {
            return Some(format!("{display} -i"));
        }
        if exact_or_inline_option_with_value(arg, &["-F", "-v", "-W"], &[]) {
            index += if has_attached_option_value(arg) { 1 } else { 2 };
            continue;
        }
        if arg.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(format!("{display} program"));
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AwkArgumentRole {
    Program,
    Data,
    Operand,
}

pub(super) fn awk_next_argument_role(args: &[String], candidate: &str) -> AwkArgumentRole {
    let mut index = 0;
    let mut program_supplied = false;
    let mut options_ended = false;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if !options_ended && arg == "--" {
            options_ended = true;
            index += 1;
            continue;
        }
        if !options_ended && exact_or_inline_option_with_value(arg, &["-f"], &["--file"]) {
            if has_attached_option_value(arg) {
                program_supplied = true;
                index += 1;
            } else if index + 1 < args.len() {
                program_supplied = true;
                index += 2;
            } else {
                return AwkArgumentRole::Operand;
            }
            continue;
        }
        if !options_ended && exact_or_inline_option_with_value(arg, &["-i"], &["--include"]) {
            if has_attached_option_value(arg) {
                index += 1;
            } else if index + 1 < args.len() {
                index += 2;
            } else {
                return AwkArgumentRole::Operand;
            }
            continue;
        }
        if !options_ended && exact_or_inline_option_with_value(arg, &["-F", "-v", "-W"], &[]) {
            if has_attached_option_value(arg) {
                index += 1;
            } else if index + 1 < args.len() {
                index += 2;
            } else {
                return AwkArgumentRole::Data;
            }
            continue;
        }
        if !options_ended && arg.starts_with('-') {
            index += 1;
            continue;
        }
        program_supplied = true;
        index += 1;
    }

    if exact_or_inline_option_with_value(candidate, &["-F", "-v", "-W"], &[])
        && has_attached_option_value(candidate)
    {
        return AwkArgumentRole::Data;
    }
    if exact_or_inline_option_with_value(candidate, &["-f"], &["--file"])
        && has_attached_option_value(candidate)
    {
        return AwkArgumentRole::Operand;
    }
    if is_awk_assignment(candidate) {
        AwkArgumentRole::Data
    } else if program_supplied {
        AwkArgumentRole::Operand
    } else {
        AwkArgumentRole::Program
    }
}

pub(super) fn awk_program_has_uninspectable_io(program: &str) -> bool {
    // ponytail: fail closed on AWK's opaque I/O hooks; single-pipe bitwise-OR is
    // also rejected unless a supported command demonstrates that it matters.
    let tokens = awk_tokens(program);
    if tokens.iter().enumerate().any(|(index, token)| match token {
        AwkToken::Identifier("system" | "ARGV" | "ARGC") => true,
        AwkToken::Symbol('|') => {
            !matches!(
                tokens.get(index.wrapping_sub(1)),
                Some(AwkToken::Symbol('|'))
            ) && !matches!(tokens.get(index + 1), Some(AwkToken::Symbol('|')))
        }
        _ => false,
    }) {
        return true;
    }

    awk_prints_to_file(&tokens)
}

fn awk_prints_to_file(tokens: &[AwkToken<'_>]) -> bool {
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token, AwkToken::Identifier("print" | "printf")) {
            continue;
        }
        let mut parentheses = 0usize;
        let mut brackets = 0usize;
        for (candidate_index, candidate) in tokens.iter().enumerate().skip(index + 1) {
            match candidate {
                AwkToken::Symbol('(') => parentheses += 1,
                AwkToken::Symbol(')') => parentheses = parentheses.saturating_sub(1),
                AwkToken::Symbol('[') => brackets += 1,
                AwkToken::Symbol(']') => brackets = brackets.saturating_sub(1),
                AwkToken::Symbol(';' | '}') | AwkToken::Newline
                    if parentheses == 0 && brackets == 0 =>
                {
                    break;
                }
                AwkToken::Symbol('>')
                    if parentheses == 0
                        && brackets == 0
                        && !matches!(
                            tokens.get(candidate_index + 1),
                            Some(AwkToken::Symbol('='))
                        ) =>
                {
                    return true;
                }
                _ => {}
            }
        }
    }
    false
}

// ponytail: inspect only getline file operands; regex text may be conservatively rejected.
// Use a real AWK parser only if that false positive affects supported commands.
pub(super) fn awk_getline_file_paths(program: &str) -> Option<Vec<String>> {
    let tokens = awk_tokens(program);
    let mut paths = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if *token != AwkToken::Identifier("getline") {
            continue;
        }
        let Some(source_index) = awk_getline_file_source_index(&tokens, index) else {
            continue;
        };
        let Some(AwkToken::StringLiteral(path, has_escapes)) = tokens.get(source_index) else {
            return None;
        };
        if *has_escapes
            || !matches!(
                tokens.get(source_index + 1),
                None | Some(
                    AwkToken::Newline
                        | AwkToken::Symbol(')')
                        | AwkToken::Symbol('}')
                        | AwkToken::Symbol(';')
                )
            )
        {
            return None;
        }
        if path.contains("://") {
            return None;
        }
        paths.push(if path.starts_with('/') {
            path.to_string()
        } else {
            format!("./{path}")
        });
    }
    Some(paths)
}

fn awk_getline_file_source_index(tokens: &[AwkToken<'_>], getline_index: usize) -> Option<usize> {
    let mut cursor = getline_index + 1;
    if matches!(tokens.get(cursor), Some(AwkToken::Symbol('<'))) {
        return Some(cursor + 1);
    }

    match tokens.get(cursor) {
        Some(AwkToken::Identifier(_)) => cursor += 1,
        Some(AwkToken::Symbol('$')) => {
            cursor += 1;
            if matches!(tokens.get(cursor), Some(AwkToken::Symbol('('))) {
                cursor = skip_awk_group(tokens, cursor, '(', ')')?;
            } else if tokens.get(cursor).is_some() {
                cursor += 1;
            } else {
                return None;
            }
        }
        Some(AwkToken::Symbol('(')) => cursor = skip_awk_group(tokens, cursor, '(', ')')?,
        _ => return None,
    }

    while matches!(tokens.get(cursor), Some(AwkToken::Symbol('['))) {
        cursor = skip_awk_group(tokens, cursor, '[', ']')?;
    }
    matches!(tokens.get(cursor), Some(AwkToken::Symbol('<'))).then_some(cursor + 1)
}

fn skip_awk_group(tokens: &[AwkToken<'_>], start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token {
            AwkToken::Symbol(ch) if *ch == open => depth += 1,
            AwkToken::Symbol(ch) if *ch == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AwkToken<'a> {
    Identifier(&'a str),
    Number(&'a str),
    StringLiteral(&'a str, bool),
    RegexLiteral,
    Symbol(char),
    Newline,
}

fn awk_tokens(source: &str) -> Vec<AwkToken<'_>> {
    let chars = source.char_indices().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut index = 0;
    while let Some(&(start, ch)) = chars.get(index) {
        if ch == '#' {
            while let Some((_, comment_char)) = chars.get(index)
                && *comment_char != '\n'
            {
                index += 1;
            }
            continue;
        }
        if ch == '"' {
            index += 1;
            let content_start = chars.get(index).map_or(source.len(), |(offset, _)| *offset);
            let mut content_end = source.len();
            let mut has_escapes = false;
            let mut escaped = false;
            while let Some((string_offset, string_char)) = chars.get(index) {
                index += 1;
                if escaped {
                    escaped = false;
                } else if *string_char == '\\' {
                    has_escapes = true;
                    escaped = true;
                } else if *string_char == '"' {
                    content_end = *string_offset;
                    break;
                }
            }
            tokens.push(AwkToken::StringLiteral(
                &source[content_start..content_end],
                has_escapes,
            ));
            continue;
        }
        if ch == '/' && awk_regex_can_start(&tokens) {
            index = skip_awk_regex(&chars, index);
            tokens.push(AwkToken::RegexLiteral);
            continue;
        }
        if ch == '\n' {
            tokens.push(AwkToken::Newline);
            index += 1;
            continue;
        }
        if ch.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            index += 1;
            while let Some((_, next)) = chars.get(index)
                && (next.is_ascii_alphanumeric() || *next == '_')
            {
                index += 1;
            }
            let end = chars.get(index).map_or(source.len(), |(offset, _)| *offset);
            tokens.push(AwkToken::Identifier(&source[start..end]));
            continue;
        }
        if ch.is_ascii_digit() {
            index += 1;
            while let Some((_, next)) = chars.get(index)
                && (next.is_ascii_alphanumeric() || matches!(*next, '_' | '.'))
            {
                index += 1;
            }
            let end = chars.get(index).map_or(source.len(), |(offset, _)| *offset);
            tokens.push(AwkToken::Number(&source[start..end]));
            continue;
        }
        tokens.push(AwkToken::Symbol(ch));
        index += 1;
    }
    tokens
}

fn awk_regex_can_start(tokens: &[AwkToken<'_>]) -> bool {
    matches!(
        tokens.last(),
        None | Some(AwkToken::Newline)
            | Some(AwkToken::Symbol(
                '(' | '{' | ';' | ',' | '~' | '!' | '=' | '&' | '|' | ':' | '?' | '<' | '>'
            ))
    )
}

fn skip_awk_regex(chars: &[(usize, char)], start: usize) -> usize {
    let mut index = start + 1;
    let mut escaped = false;
    let mut in_character_class = false;
    while let Some((_, ch)) = chars.get(index) {
        index += 1;
        if escaped {
            escaped = false;
        } else if *ch == '\\' {
            escaped = true;
        } else if *ch == '[' {
            in_character_class = true;
        } else if *ch == ']' {
            in_character_class = false;
        } else if *ch == '/' && !in_character_class {
            break;
        }
    }
    index
}

fn is_awk_assignment(arg: &str) -> bool {
    let Some((name, _)) = arg.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn shell_query_flag(arg: &str) -> bool {
    matches!(arg, "--help" | "--version")
}

fn python_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "--version") || arg.starts_with("-V")
}

fn node_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version")
}

fn perl_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help") || arg.starts_with("-v") || arg.starts_with("-V")
}

fn ruby_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version")
}

fn lua_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version")
}

fn php_query_flag(arg: &str) -> bool {
    matches!(arg, "-h" | "--help" | "-v" | "--version" | "-i" | "-m")
}

fn awk_query_flag(arg: &str) -> bool {
    matches!(arg, "--help" | "--version" | "-Wversion" | "-Whelp")
}

fn is_shell_eval_wrapper(command: &str, flag: &str) -> bool {
    matches!(command, "sh" | "bash" | "dash" | "zsh" | "ksh")
        && shell_flag_contains_short_option(flag, 'c')
}

fn is_code_eval_wrapper(command: &str, flag: &str) -> bool {
    match command {
        "python" | "python3" => shell_flag_contains_short_option(flag, 'c'),
        "node" => {
            shell_flag_contains_short_option(flag, 'e')
                || shell_flag_contains_short_option(flag, 'p')
                || flag == "--print"
        }
        "perl" => {
            shell_flag_contains_short_option(flag, 'e')
                || shell_flag_contains_short_option(flag, 'E')
        }
        "ruby" | "lua" => shell_flag_contains_short_option(flag, 'e'),
        "php" => shell_flag_contains_short_option(flag, 'r'),
        _ => false,
    }
}

pub(super) fn leading_eval_flag<'a>(command: &str, args: &'a [String]) -> Option<&'a str> {
    match command {
        "sh" | "bash" | "dash" | "zsh" | "ksh" => scan_leading_args(
            args,
            |arg| is_shell_eval_wrapper("sh", arg),
            shell_wrapper_advance,
        ),
        "python" | "python3" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("python3", arg),
            python_wrapper_advance,
        ),
        "node" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("node", arg),
            node_wrapper_advance,
        ),
        "perl" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("perl", arg),
            perl_wrapper_advance,
        ),
        "ruby" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("ruby", arg),
            ruby_wrapper_advance,
        ),
        "lua" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("lua", arg),
            lua_wrapper_advance,
        ),
        "php" => scan_leading_args(
            args,
            |arg| is_code_eval_wrapper("php", arg),
            php_wrapper_advance,
        ),
        _ => None,
    }
}

fn scan_leading_args<F, G>(args: &[String], matches_eval: F, advance: G) -> Option<&str>
where
    F: Fn(&str) -> bool,
    G: Fn(&str) -> Option<usize>,
{
    let mut index = 0;
    while let Some(arg) = args.get(index).map(|arg| arg.as_str()) {
        if arg == "--" {
            break;
        }
        if matches_eval(arg) {
            return Some(arg);
        }
        index += advance(arg)?;
    }
    None
}

fn shell_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-o", "+o", "-O", "+O"],
        &["--rcfile", "--init-file"],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') || arg.starts_with('+') {
        Some(1)
    } else {
        None
    }
}

fn python_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-W", "-X"], &["--check-hash-based-pycs"]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if matches!(arg, "-m" | "--module" | "-") {
        None
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn node_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-r", "-C"],
        &[
            "--require",
            "--loader",
            "--experimental-loader",
            "--import",
            "--watch-path",
            "--conditions",
            "--input-type",
            "--inspect",
            "--inspect-brk",
            "--inspect-port",
            "--openssl-config",
            "--redirect-warnings",
            "--trace-event-categories",
            "--trace-event-file-pattern",
            "--diagnostic-dir",
            "--icu-data-dir",
            "--title",
        ],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn perl_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-I", "-M", "-m"], &[]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn ruby_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(
        arg,
        &["-C", "-E", "-F", "-I", "-r"],
        &["--enable", "--disable", "--encoding"],
    ) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn lua_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-l"], &[]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn php_wrapper_advance(arg: &str) -> Option<usize> {
    if exact_or_inline_option_with_value(arg, &["-c", "-d", "-z"], &["--define"]) {
        Some(if has_attached_option_value(arg) { 1 } else { 2 })
    } else if matches!(arg, "-f" | "--file") {
        None
    } else if arg.starts_with('-') {
        Some(1)
    } else {
        None
    }
}

fn shell_flag_contains_short_option(flag: &str, option: char) -> bool {
    if let Some(rest) = flag
        .strip_prefix("--")
        .map(|rest| rest.split_once('=').map_or(rest, |(name, _)| name))
    {
        return matches!(
            (rest, option),
            ("command", 'c') | ("eval", 'e') | ("print", 'p') | ("run", 'r')
        );
    }

    flag.starts_with('-') && flag.chars().skip(1).any(|ch| ch == option)
}
