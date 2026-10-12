pub(super) fn exact_or_inline_option_with_value(arg: &str, short: &[&str], long: &[&str]) -> bool {
    short
        .iter()
        .any(|flag| arg == *flag || arg.starts_with(flag))
        || long
            .iter()
            .any(|flag| arg == *flag || arg.starts_with(&format!("{flag}=")))
}

pub(super) fn has_attached_option_value(arg: &str) -> bool {
    arg.contains('=') || (arg.starts_with('-') && !arg.starts_with("--") && arg.len() > 2)
}

pub(in crate::tools) fn cargo_subcommand(args: &[String]) -> Option<&str> {
    cargo_command_options(args)
        .0
        .map(|index| args[index].as_str())
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(in crate::tools) fn cargo_subcommand_index(args: &[String]) -> Option<usize> {
    cargo_command_options(args).0
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(in crate::tools) fn cargo_global_query(args: &[String]) -> bool {
    cargo_command_options(args).1
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(in crate::tools) fn cargo_global_help(args: &[String]) -> bool {
    cargo_command_options(args).2
}

fn cargo_command_options(args: &[String]) -> (Option<usize>, bool, bool) {
    let mut words = args.iter().enumerate();
    if args.first().is_some_and(|word| word.starts_with('+')) {
        words.next();
    }
    let mut query = false;
    let mut help = false;
    while let Some((index, word)) = words.next() {
        let short = word.strip_prefix('-').filter(|flags| {
            flags
                .chars()
                .all(|flag| matches!(flag, 'v' | 'q' | 'h' | 'V'))
        });
        if word == "--" {
            return (words.next().map(|(index, _)| index), query, help);
        }
        if matches!(word.as_str(), "--config" | "--color" | "-C" | "-Z") {
            if words.next().is_none() {
                return (None, query, help);
            }
        } else if !word.starts_with('-') {
            return (Some(index), query, help);
        } else if matches!(
            word.as_str(),
            "--help" | "-h" | "--version" | "-V" | "--list"
        ) || short.is_some_and(|flags| flags.contains('h') || flags.contains('V'))
        {
            query = true;
            help |= matches!(word.as_str(), "--help" | "-h")
                || short.is_some_and(|flags| flags.contains('h'));
        }
    }
    (None, query, help)
}
