//! Build-time version, release channel, and commit metadata injection for `alan`.

use std::env;

fn main() {
    println!("cargo:rerun-if-env-changed=ALAN_BUILD_VERSION");
    println!("cargo:rerun-if-env-changed=ALAN_BUILD_CHANNEL");
    println!("cargo:rerun-if-env-changed=ALAN_BUILD_COMMIT");

    let pkg_version =
        env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION must be set by Cargo");

    let version = match env::var("ALAN_BUILD_VERSION") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                pkg_version
            } else {
                validate_semver(trimmed);
                trimmed.to_string()
            }
        }
        Err(env::VarError::NotPresent) => pkg_version,
        Err(err) => panic!("failed to read ALAN_BUILD_VERSION: {err}"),
    };

    let channel = match env::var("ALAN_BUILD_CHANNEL") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                "stable"
            } else {
                match trimmed {
                    "stable" => "stable",
                    "beta" => "beta",
                    other => {
                        panic!("invalid ALAN_BUILD_CHANNEL `{other}`; expected `stable` or `beta`")
                    }
                }
            }
        }
        Err(env::VarError::NotPresent) => "stable",
        Err(err) => panic!("failed to read ALAN_BUILD_CHANNEL: {err}"),
    };

    let commit = match env::var("ALAN_BUILD_COMMIT") {
        Ok(raw) => {
            let trimmed = raw.trim();
            assert!(
                !trimmed.chars().any(char::is_whitespace),
                "ALAN_BUILD_COMMIT must not contain whitespace: `{trimmed}`"
            );
            trimmed.to_string()
        }
        Err(env::VarError::NotPresent) => String::new(),
        Err(err) => panic!("failed to read ALAN_BUILD_COMMIT: {err}"),
    };

    let mut metadata = Vec::new();
    if channel != "stable" {
        metadata.push(channel);
    }
    if !commit.is_empty() {
        metadata.push(commit.as_str());
    }
    let long_version = if metadata.is_empty() {
        version.clone()
    } else {
        format!("{version} ({})", metadata.join(", "))
    };

    println!("cargo:rustc-env=ALAN_VERSION={version}");
    println!("cargo:rustc-env=ALAN_RELEASE_CHANNEL={channel}");
    println!("cargo:rustc-env=ALAN_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=ALAN_LONG_VERSION={long_version}");
}

fn validate_semver(raw: &str) {
    assert!(
        !raw.starts_with('v') && !raw.starts_with('V'),
        "ALAN_BUILD_VERSION must not start with 'v': `{raw}`"
    );
    assert!(
        !raw.chars().any(char::is_whitespace),
        "ALAN_BUILD_VERSION must not contain whitespace: `{raw}`"
    );

    let (core_and_pre, build_opt) = match raw.split_once('+') {
        Some((head, build)) => (head, Some(build)),
        None => (raw, None),
    };
    if let Some(build) = build_opt {
        validate_dot_segments(build, "build metadata", false, raw);
    }

    let (core, pre_opt) = match core_and_pre.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (core_and_pre, None),
    };

    let mut parts = core.split('.');
    validate_numeric_part(parts.next().unwrap_or(""), "major", raw);
    validate_numeric_part(parts.next().unwrap_or(""), "minor", raw);
    validate_numeric_part(parts.next().unwrap_or(""), "patch", raw);
    assert!(
        parts.next().is_none(),
        "invalid ALAN_BUILD_VERSION `{raw}`: expected MAJOR.MINOR.PATCH"
    );

    if let Some(pre) = pre_opt {
        validate_dot_segments(pre, "pre-release", true, raw);
    }
}

fn validate_numeric_part(segment: &str, label: &str, full: &str) {
    assert!(
        !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit()),
        "invalid {label} component in ALAN_BUILD_VERSION `{full}`"
    );
    assert!(
        segment.len() == 1 || !segment.starts_with('0'),
        "leading zero not allowed in {label} component of ALAN_BUILD_VERSION `{full}`"
    );
}

fn validate_dot_segments(raw: &str, label: &str, forbid_numeric_leading_zero: bool, full: &str) {
    assert!(
        !raw.is_empty(),
        "empty {label} section in ALAN_BUILD_VERSION `{full}`"
    );
    for seg in raw.split('.') {
        assert!(
            !seg.is_empty(),
            "empty {label} identifier in ALAN_BUILD_VERSION `{full}`"
        );
        assert!(
            seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid {label} identifier `{seg}` in ALAN_BUILD_VERSION `{full}`"
        );
        if forbid_numeric_leading_zero
            && seg.bytes().all(|b| b.is_ascii_digit())
            && seg.len() > 1
            && seg.starts_with('0')
        {
            panic!(
                "leading zero not allowed in numeric {label} identifier `{seg}` of ALAN_BUILD_VERSION `{full}`"
            );
        }
    }
}
