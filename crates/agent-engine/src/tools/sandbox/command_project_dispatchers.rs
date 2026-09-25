pub(super) fn is_project_code_dispatcher(command: &str, args: &[String]) -> bool {
    if matches!(command, "python" | "python3")
        && args.windows(2).any(|pair| {
            matches!(pair[0].as_str(), "-m" | "--module") && one_of(&pair[1], "pytest unittest")
        })
    {
        return true;
    }
    if command == "pytest" {
        return !args.iter().any(|arg| one_of(arg, "--help -h --version"));
    }
    let Some(subcommand) = package_manager_subcommand(command, args) else {
        return false;
    };

    // ponytail: this bounded classifier defers to kernel read confinement as the upgrade path.
    match command {
        "npm" => one_of(
            subcommand,
            "run run-script rum urn exec x explore install i ci add update up rebuild restart start stop test tst install-test install-ci-test init create pack publish version",
        ),
        "npx" | "pnpx" | "bunx" => true,
        "pnpm" => {
            one_of(
                subcommand,
                "run exec dlx test t start install i add update up rebuild create",
            ) || !one_of(
                subcommand,
                "--version -v --help help root store list ls view info why config audit licenses server",
            )
        }
        "yarn" => {
            one_of(
                subcommand,
                "run exec dlx test start install add upgrade upgrade-interactive create node version workspace",
            ) || !one_of(
                subcommand,
                "--version -v --help help info why list cache config licenses",
            )
        }
        "bun" => {
            one_of(subcommand, "run x test install i add update upgrade")
                || !one_of(subcommand, "--version -v --help help pm")
        }
        "deno" => one_of(subcommand, "run task test"),
        "cargo" => {
            one_of(
                subcommand,
                "build check run test bench clippy doc install package publish rustc",
            ) || !one_of(
                subcommand,
                "--version -V --help help clean fmt metadata tree search info update fetch vendor generate-lockfile locate-project read-manifest verify-project new init add remove rm",
            )
        }
        "go" => !one_of(
            subcommand,
            "--version -V --help help version env doc fmt list",
        ),
        // SwiftPM runs manifests/plugins, and callers can disable its subprocess sandbox.
        "swift" => subcommand != "help",
        _ => false,
    }
}

fn one_of(value: &str, choices: &str) -> bool {
    choices.split_whitespace().any(|v| v == value)
}

fn package_manager_subcommand<'a>(command: &str, args: &'a [String]) -> Option<&'a str> {
    let value_options = match command {
        "npm" => {
            "--prefix --workspace -w --userconfig --registry --cache --loglevel --otp --script-shell --location"
        }
        "pnpm" => "--dir -C --filter -F --config --registry --store-dir",
        "yarn" => "--cwd --cache-folder --modules-folder",
        "bun" => "--cwd --config",
        "deno" => "--config --import-map --lock --cert",
        "cargo" => {
            "--color --config --manifest-path --target --target-dir --message-format --explain"
        }
        "go" => "-C",
        "swift" => {
            "--package-path --cache-path --config-path --security-path --scratch-path --swift-sdks-path --toolset --pkg-config-path"
        }
        _ => "",
    };

    let mut index = 0;
    while let Some(argument) = args.get(index).map(String::as_str) {
        if argument == "--" {
            return args.get(index + 1).map(String::as_str);
        }
        if one_of(argument, value_options) {
            index += 2;
            continue;
        }
        if argument.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(argument);
    }
    None
}
