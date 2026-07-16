use super::command_options::{exact_or_inline_option_with_value, has_attached_option_value};

pub(super) fn opaque_command_dispatcher_display(
    display: &str,
    command: &str,
    args: &[String],
) -> Option<String> {
    if command == "xargs" {
        return Some(display.to_string());
    }
    (command == "find")
        .then_some(())
        .and_then(|()| find_dispatch_clause(args))
        .map(|clause| format!("{display} {clause}"))
}

fn find_dispatch_clause(args: &[String]) -> Option<&'static str> {
    const FIND_DISPATCH_FLAGS: [&str; 4] = ["-exec", "-execdir", "-ok", "-okdir"];

    args.iter().enumerate().find_map(|(index, arg)| {
        let flag = FIND_DISPATCH_FLAGS
            .iter()
            .copied()
            .find(|flag| *flag == arg)?;
        let tail = &args[index + 1..];
        let first_child_arg = tail.first()?;
        if first_child_arg.starts_with('-') {
            return None;
        }
        tail.iter()
            .any(|candidate| candidate == ";" || candidate == "+")
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
            let module = args.get(index + 1).map(|value| value.as_str());
            if module.is_some_and(is_safe_python_module_runner) {
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
