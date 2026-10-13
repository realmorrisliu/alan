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
        if word == "--" {
            return (words.next().map(|(index, _)| index), query, help);
        }
        if matches!(word.as_str(), "--config" | "--color" | "-C" | "-Z") {
            if words.next().is_none() {
                return (None, query, help);
            }
        } else if !word.starts_with('-') {
            return (Some(index), query, help);
        } else if matches!(word.as_str(), "--help" | "--version" | "--list") {
            query = true;
            help |= word == "--help";
        } else if word.starts_with('-') && !word.starts_with("--") {
            let Some(short) = cargo_short_options(word) else {
                return (None, help, help);
            };
            query |= short.query;
            help |= short.help;
            if short.value.is_some() && !short.attached_value && words.next().is_none() {
                return (None, query, help);
            }
        }
    }
    (None, query, help)
}

#[derive(Default)]
struct CargoShortOptions {
    query: bool,
    help: bool,
    value: Option<char>,
    attached_value: bool,
}

fn cargo_short_options(word: &str) -> Option<CargoShortOptions> {
    let mut options = CargoShortOptions::default();
    let mut flags = word.strip_prefix('-')?.chars();
    while let Some(flag) = flags.next() {
        match flag {
            'v' | 'q' => {}
            'V' => options.query = true,
            'h' => {
                // Cargo help exits while parsing, before later flags or their values.
                options.query = true;
                options.help = true;
                break;
            }
            'C' | 'Z' => {
                options.value = Some(flag);
                options.attached_value = !flags.as_str().is_empty();
                break;
            }
            _ => return None,
        }
    }
    Some(options)
}

#[cfg(any(target_os = "linux", all(test, unix)))]
pub(in crate::tools) fn cargo_changes_directory(args: &[String]) -> bool {
    let mut words = args.iter().take_while(|word| word.as_str() != "--");
    while let Some(word) = words.next() {
        if matches!(
            word.as_str(),
            "--config" | "--color" | "--manifest-path" | "--features" | "-F"
        ) {
            words.next();
        } else if word.starts_with('-')
            && !word.starts_with("--")
            && let Some(short) = cargo_short_options(word)
        {
            if short.value == Some('C') {
                return true;
            }
            if short.value.is_some() && !short.attached_value {
                words.next();
            }
        }
    }
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn mixed_short_queries_respect_value_arity_and_help_priority() {
        for (flag, query, help) in [
            ("-hZunstable-options", true, true),
            ("-VZunstable-options", true, false),
            ("-vVZunstable-options", true, false),
            ("-hX", true, true),
            ("-Xh", false, false),
            ("-VH", false, false),
            ("-ZhV", false, false),
            ("-ChV", false, false),
            ("-vZ--help", false, false),
            ("-VCmissing", true, false),
            ("-hCmissing", true, true),
        ] {
            let args = [flag, "fmt"].map(String::from);
            assert_eq!(cargo_global_query(&args), query, "{flag}");
            assert_eq!(cargo_global_help(&args), help, "{flag}");
        }
        for args in [["-Z", "hV", "fmt"], ["-C", "hV", "fmt"]] {
            let args = args.map(String::from);
            assert!(!cargo_global_query(&args));
            assert_eq!(cargo_subcommand(&args), Some("fmt"));
        }
    }

    #[test]
    fn directory_changes_include_clusters_but_exclude_option_values_and_payload() {
        for (words, expected) in [
            (vec!["-vCchild", "test"], true),
            (vec!["-VCchild", "custom"], true),
            (vec!["test", "-vCchild"], true),
            (vec!["test", "-C", "child"], true),
            (vec!["-ZCchild", "test"], false),
            (vec!["-vZ", "-Cchild", "test"], false),
            (vec!["--color", "-Cchild", "test"], false),
            (vec!["--config", "-Cchild", "test"], false),
            (vec!["--config=-Cchild", "test"], false),
            (vec!["test", "--features", "-Cchild"], false),
            (vec!["test", "--", "-vCchild"], false),
            (vec!["-hCchild", "test"], false),
        ] {
            let args: Vec<_> = words.iter().map(|word| String::from(*word)).collect();
            assert_eq!(cargo_changes_directory(&args), expected, "{words:?}");
        }
    }
}
