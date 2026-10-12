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
    let mut words = args.iter();
    if args.first().is_some_and(|word| word.starts_with('+')) {
        words.next();
    }
    while let Some(word) = words.next() {
        if word == "--" {
            return words.next().map(String::as_str);
        }
        if matches!(word.as_str(), "--config" | "--color" | "-C" | "-Z") {
            words.next()?;
        } else if !word.starts_with('-') {
            return Some(word);
        }
    }
    None
}
