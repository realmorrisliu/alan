use super::*;

#[test]
fn standalone_cd_parser_accepts_one_literal_path_and_leaves_scripts_alone() {
    assert_eq!(
        parse_standalone_cd("cd 'folder with spaces'").unwrap(),
        Some(PathBuf::from("folder with spaces"))
    );
    assert_eq!(
        parse_standalone_cd("cd folder\\ with\\ spaces").unwrap(),
        Some(PathBuf::from("folder with spaces"))
    );
    assert_eq!(
        parse_standalone_cd("cd '/mnt/project/src'").unwrap(),
        Some(PathBuf::from("/mnt/project/src"))
    );
    assert_eq!(
        parse_standalone_cd("cd 'work;tree'").unwrap(),
        Some(PathBuf::from("work;tree"))
    );
    assert_eq!(
        parse_standalone_cd("cd '~'").unwrap(),
        Some(PathBuf::from("~"))
    );
    assert_eq!(
        parse_standalone_cd("cd '$HOME'").unwrap(),
        Some(PathBuf::from("$HOME"))
    );
    assert_eq!(
        parse_standalone_cd("cd '*.rs'").unwrap(),
        Some(PathBuf::from("*.rs"))
    );
    assert_eq!(parse_standalone_cd("cd src && pwd").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd src &").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd src > result.txt").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd src\nmake").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd src\n$EDITOR").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd $(pwd) && make").unwrap(), None);
    assert_eq!(
        parse_standalone_cd("cd `printf src; printf x >&2` && make").unwrap(),
        None
    );
    assert_eq!(
        parse_standalone_cd("cd '`literal;name`'").unwrap(),
        Some(PathBuf::from("`literal;name`"))
    );
    assert_eq!(parse_standalone_cd("cd {src,test} && make").unwrap(), None);
    assert_eq!(parse_standalone_cd("printf 'cd src'").unwrap(), None);
    assert_eq!(parse_standalone_cd("printf '").unwrap(), None);
}

#[test]
fn standalone_cd_parser_rejects_unsupported_forms_explicitly() {
    for command in [
        "cd",
        "cd ''",
        "cd \"\"",
        "cd one two",
        "cd -",
        "cd -P",
        "cd --",
        "cd ~/project",
        "cd $HOME",
        "cd ${HOME}",
        "cd $(pwd)",
        "cd {src,test}",
        "cd `pwd`",
        "cd `printf /mnt/project; printf x >&2`",
        "cd `printf src\nprintf x >&2`",
        "cd `printf src | cat`",
        "cd $(printf `printf src; printf x >&2`)",
        "cd *.rs",
    ] {
        assert!(
            parse_standalone_cd(command).is_err(),
            "expected standalone cd to reject {command:?}"
        );
    }
}
