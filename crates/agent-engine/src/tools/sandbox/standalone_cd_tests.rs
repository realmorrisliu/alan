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
    assert_eq!(parse_standalone_cd("cd src && pwd").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd src &").unwrap(), None);
    assert_eq!(parse_standalone_cd("cd src > result.txt").unwrap(), None);
    assert_eq!(parse_standalone_cd("printf 'cd src'").unwrap(), None);
}

#[test]
fn standalone_cd_parser_rejects_unsupported_forms_explicitly() {
    for command in [
        "cd",
        "cd one two",
        "cd -",
        "cd ~/project",
        "cd $HOME",
        "cd `pwd`",
        "cd *.rs",
    ] {
        assert!(
            parse_standalone_cd(command).is_err(),
            "expected standalone cd to reject {command:?}"
        );
    }
}
