use super::*;

#[test]
fn leading_system_provider_does_not_inspect_unrelated_rustup_settings() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    for name in ["cargo", "rustc"] {
        std::fs::write(system.join(name), b"\x7fELFordinary system executable").unwrap();
        std::fs::set_permissions(system.join(name), std::fs::Permissions::from_mode(0o755))
            .unwrap();
    }
    std::fs::remove_file(home.join("settings.toml")).unwrap();
    let path = format!("{}:{}", system.display(), proxies.display());
    for command in ["cargo", "rustc"] {
        let environment = discover(
            &path,
            &home,
            None,
            Some((
                &input(&project),
                &[vec![command.into(), "--version".into()]],
            )),
        )
        .unwrap()
        .unwrap();
        assert!(environment.toolchain_mounts.is_empty());
        assert!(environment.metadata_hashes.is_empty());
        revalidate(&environment).unwrap();
    }
}

#[test]
fn provider_order_permissions_aliases_and_explicit_paths_are_preserved() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    let path = format!("{}:{}", system.display(), proxies.display());
    let commands = vec![vec!["cargo".into(), "--version".into()]];
    let inspect = |path: &str, commands: &[Vec<String>]| {
        discover(path, &home, None, Some((&input(&project), commands)))
            .unwrap()
            .unwrap()
    };
    let candidate = system.join("cargo");
    std::fs::write(&candidate, b"\x7fELFordinary cargo").unwrap();
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(inspect(&path, &commands).toolchain_mounts.len(), 1);
    std::fs::remove_file(&candidate).unwrap();
    std::fs::create_dir(&candidate).unwrap();
    assert_eq!(inspect(&path, &commands).toolchain_mounts.len(), 1);
    std::fs::remove_dir(&candidate).unwrap();
    std::fs::hard_link(proxies.join("rustup"), &candidate).unwrap();
    assert_eq!(inspect(&path, &commands).toolchain_mounts.len(), 1);
    std::fs::remove_file(&candidate).unwrap();
    symlink(proxies.join("rustup"), &candidate).unwrap();
    assert!(discover(&path, &home, None, Some((&input(&project), &commands))).is_err());
    std::fs::remove_file(&candidate).unwrap();
    std::fs::write(&candidate, b"\x7fELFordinary cargo").unwrap();
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(inspect(&path, &commands).toolchain_mounts.is_empty());
    assert_eq!(
        inspect(
            &format!("{}:{}", proxies.display(), system.display()),
            &commands
        )
        .toolchain_mounts
        .len(),
        1
    );
    for (executable, expected) in [(candidate, 0), (proxies.join("cargo"), 1)] {
        let commands = vec![vec![
            executable.to_str().unwrap().into(),
            "--version".into(),
        ]];
        assert_eq!(inspect(&path, &commands).toolchain_mounts.len(), expected);
    }
    let ordinary = project.parent().unwrap().join("ordinary");
    std::fs::create_dir(&ordinary).unwrap();
    let other = ordinary.join("rustup");
    std::fs::write(&other, b"\x7fELFordinary outer rustup").unwrap();
    std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        inspect(
            &path,
            &[vec![
                other.to_str().unwrap().into(),
                "run".into(),
                "missing".into(),
                "cargo".into(),
                "test".into()
            ]]
        )
        .toolchain_mounts
        .is_empty()
    );
}

#[test]
fn provider_changes_refuse_before_execution() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    let path = format!("{}:{}", system.display(), proxies.display());
    let commands = vec![vec!["cargo".into(), "--version".into()]];
    let inspect = || {
        discover(&path, &home, None, Some((&input(&project), &commands)))
            .unwrap()
            .unwrap()
    };
    let old = inspect();
    let candidate = system.join("cargo");
    std::fs::write(&candidate, b"\x7fELFordinary cargo").unwrap();
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(revalidate(&old).unwrap_err().contains("provider changed"));
    let current = inspect();
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        revalidate(&current)
            .unwrap_err()
            .contains("provider changed")
    );
    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755)).unwrap();
    let current = inspect();
    std::fs::write(&candidate, b"\x7fELFchanged cargo").unwrap();
    assert!(
        revalidate(&current)
            .unwrap_err()
            .contains("provider changed")
    );
}

#[test]
fn unresolved_context_and_mixed_compilers_refuse_before_effects() {
    let (_fixture, proxies, home, project) = installation();
    let inspect = |commands: &[Vec<String>]| {
        discover(
            proxies.to_str().unwrap(),
            &home,
            None,
            Some((&input(&project), commands)),
        )
    };
    for words in [
        vec!["command", "-p", "cargo", "test"],
        vec!["env", "-i", "cargo", "test"],
        vec!["env", "-0uPATH", "cargo", "test"],
        vec!["env", "--unset=RUSTUP_TOOLCHAIN", "cargo", "test"],
        vec!["env", "-uRUSTUP_HOME", "cargo", "test"],
        vec!["env", "-C", "child", "cargo", "test"],
        vec!["env", "--chdir=child", "cargo", "test"],
        vec!["exec", "-c", "cargo", "test"],
        vec!["./cargo", "test"],
    ] {
        let commands = vec![words.iter().map(|s| (*s).into()).collect()];
        assert!(inspect(&commands).is_err(), "{words:?}");
    }
    for words in [
        vec!["env", "-u", "NO_COLOR", "cargo", "test"],
        vec!["env", "--unset=RUSTFLAGS", "cargo", "test"],
        vec!["env", "--", "RUSTUP_TOOLCHAIN=1.97.0", "cargo", "test"],
        vec!["command", "cargo", "test"],
        vec!["exec", "cargo", "test"],
    ] {
        let commands = vec![words.iter().map(|s| (*s).into()).collect()];
        inspect(&commands).unwrap_or_else(|error| panic!("{words:?}: {error}"));
    }
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    std::fs::write(system.join("cargo"), b"\x7fELFordinary cargo").unwrap();
    std::fs::set_permissions(system.join("cargo"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", system.display(), proxies.display());
    assert!(
        discover(
            &path,
            &home,
            None,
            Some((&input(&project), &[vec!["cargo".into(), "test".into()]]))
        )
        .unwrap_err()
        .contains("mixed system Cargo")
    );
}

#[test]
fn provider_alias_chains_cannot_leave_the_projected_root() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    let path = format!("{}:{}", system.display(), proxies.display());
    let commands = vec![vec!["cargo".into(), "--version".into()]];
    symlink("next", system.join("cargo")).unwrap();
    symlink(proxies.join("rustup"), system.join("next")).unwrap();
    assert!(discover(&path, &home, None, Some((&input(&project), &commands))).is_err());
    std::fs::remove_file(system.join("cargo")).unwrap();
    symlink("nested/rustup", system.join("cargo")).unwrap();
    symlink(&proxies, system.join("nested")).unwrap();
    assert!(discover(&path, &home, None, Some((&input(&project), &commands))).is_err());
}

#[test]
fn byte_identical_replacement_cannot_change_proxy_classification() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    let cargo = system.join("cargo");
    std::fs::hard_link(proxies.join("rustup"), &cargo).unwrap();
    let path = format!("{}:{}", system.display(), proxies.display());
    let commands = vec![vec!["cargo".into(), "--version".into()]];
    let environment = discover(&path, &home, None, Some((&input(&project), &commands)))
        .unwrap()
        .unwrap();
    let contents = std::fs::read(&cargo).unwrap();
    std::fs::remove_file(&cargo).unwrap();
    std::fs::write(&cargo, &contents).unwrap();
    std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        revalidate(&environment)
            .unwrap_err()
            .contains("provider changed")
    );
}

#[test]
fn system_cargo_freezes_its_implicit_compiler_lookup() {
    let (_fixture, proxies, home, project) = installation();
    let system = project.parent().unwrap().join("system");
    std::fs::create_dir(&system).unwrap();
    for name in ["cargo", "rustc", "rustdoc"] {
        executable(&system.join(name));
    }
    let path = format!("{}:{}", system.display(), proxies.display());
    let commands = vec![vec!["cargo".into(), "test".into()]];
    let environment = discover(&path, &home, None, Some((&input(&project), &commands)))
        .unwrap()
        .unwrap();
    assert!(
        environment
            .command_providers
            .iter()
            .any(|binding| binding.executable == "rustc")
    );
    std::fs::remove_file(system.join("rustc")).unwrap();
    assert!(
        revalidate(&environment)
            .unwrap_err()
            .contains("provider changed")
    );
}
