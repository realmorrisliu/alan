//! Semantic version parsing, comparison, and supported release target mapping.

use anyhow::{Context, Result, bail, ensure};
use std::cmp::Ordering;
use std::fmt;

/// Release target triples supported by official `alan` builds and self-updates.
pub const SUPPORTED_TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
];

/// Pre-release identifier segment following SemVer 2.0.0 ordering rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreReleaseId {
    /// Purely numeric identifier (compared numerically, sorts lower than alphanumeric).
    Numeric(u64),
    /// Alphanumeric or hyphenated identifier (compared ASCII-lexicographically).
    AlphaNumeric(String),
}

impl Ord for PreReleaseId {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(a), Self::Numeric(b)) => a.cmp(b),
            (Self::Numeric(_), Self::AlphaNumeric(_)) => Ordering::Less,
            (Self::AlphaNumeric(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::AlphaNumeric(a), Self::AlphaNumeric(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for PreReleaseId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Parsed SemVer 2.0.0 version (build metadata is validated but ignored for precedence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemVer {
    /// Major version component.
    pub major: u64,
    /// Minor version component.
    pub minor: u64,
    /// Patch version component.
    pub patch: u64,
    /// Optional pre-release identifiers; empty indicates a stable release.
    pub pre: Vec<PreReleaseId>,
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)) {
            Ordering::Equal => match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            },
            non_eq => non_eq,
        }
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for SemVer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            f.write_str("-")?;
            for (idx, part) in self.pre.iter().enumerate() {
                if idx > 0 {
                    f.write_str(".")?;
                }
                match part {
                    PreReleaseId::Numeric(n) => write!(f, "{n}")?,
                    PreReleaseId::AlphaNumeric(s) => f.write_str(s)?,
                }
            }
        }
        Ok(())
    }
}

/// Parse a SemVer 2.0.0 string with optional leading `v` or `V` tag prefix.
pub fn parse_semver(raw: &str) -> Result<SemVer> {
    let trimmed = raw.trim();
    ensure!(!trimmed.is_empty(), "version string cannot be empty");
    let without_v = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);

    let (core_and_pre, build_opt) = match without_v.split_once('+') {
        Some((head, build)) => (head, Some(build)),
        None => (without_v, None),
    };
    if let Some(build) = build_opt {
        validate_dot_segments(build, "build metadata", false)?;
    }

    let (core, pre_opt) = match core_and_pre.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (core_and_pre, None),
    };

    let mut parts = core.split('.');
    let major = parse_numeric_part(parts.next().unwrap_or(""), "major", raw)?;
    let minor = parse_numeric_part(parts.next().unwrap_or(""), "minor", raw)?;
    let patch = parse_numeric_part(parts.next().unwrap_or(""), "patch", raw)?;
    ensure!(
        parts.next().is_none(),
        "invalid semantic version `{raw}`: expected MAJOR.MINOR.PATCH"
    );

    let mut pre = Vec::new();
    if let Some(pre_raw) = pre_opt {
        let segments = validate_dot_segments(pre_raw, "pre-release", true)?;
        for seg in segments {
            if seg.bytes().all(|b| b.is_ascii_digit()) {
                let n = seg
                    .parse::<u64>()
                    .with_context(|| format!("invalid pre-release number in `{raw}`"))?;
                pre.push(PreReleaseId::Numeric(n));
            } else {
                pre.push(PreReleaseId::AlphaNumeric(seg.to_string()));
            }
        }
    }

    Ok(SemVer {
        major,
        minor,
        patch,
        pre,
    })
}

fn parse_numeric_part(segment: &str, label: &str, full: &str) -> Result<u64> {
    ensure!(
        !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit()),
        "invalid {label} component in semantic version `{full}`"
    );
    ensure!(
        segment.len() == 1 || !segment.starts_with('0'),
        "leading zero not allowed in {label} component of `{full}`"
    );
    segment
        .parse::<u64>()
        .with_context(|| format!("invalid {label} component in `{full}`"))
}

fn validate_dot_segments<'a>(
    raw: &'a str,
    label: &str,
    forbid_numeric_leading_zero: bool,
) -> Result<Vec<&'a str>> {
    ensure!(!raw.is_empty(), "{label} section cannot be empty");
    let mut out = Vec::new();
    for seg in raw.split('.') {
        ensure!(
            !seg.is_empty(),
            "{label} section `{raw}` contains empty identifier"
        );
        ensure!(
            seg.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "{label} identifier `{seg}` contains invalid characters"
        );
        if forbid_numeric_leading_zero
            && seg.bytes().all(|b| b.is_ascii_digit())
            && seg.len() > 1
            && seg.starts_with('0')
        {
            bail!("leading zero not allowed in numeric {label} identifier `{seg}`");
        }
        out.push(seg);
    }
    Ok(out)
}

/// Strip an optional `v` prefix after validating SemVer syntax.
pub fn normalize_version_str(raw: &str) -> Result<String> {
    let parsed = parse_semver(raw)?;
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let _ = parsed;
    Ok(stripped.to_string())
}

/// Format a version or tag string with a single lowercase `v` prefix for display.
pub fn display_tag(version_or_tag: &str) -> String {
    let trimmed = version_or_tag.trim();
    let bare = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    format!("v{bare}")
}

/// Return `true` when `candidate` is strictly newer than `current` under SemVer 2.0.0.
pub fn is_newer_version(current: &str, candidate: &str) -> Result<bool> {
    let current = parse_semver(current)?;
    let candidate = parse_semver(candidate)?;
    Ok(candidate > current)
}

/// Map `(os, arch)` from `std::env::consts` to one of the supported release target triples.
pub fn target_triple_for(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Ok("aarch64-unknown-linux-gnu"),
        _ => bail!(
            "unsupported platform `{os}/{arch}` for alan self-update; supported targets: {}",
            SUPPORTED_TARGETS.join(", ")
        ),
    }
}

/// Return the Rust target triple for the currently running `alan` executable.
pub fn current_target_triple() -> Result<&'static str> {
    target_triple_for(std::env::consts::OS, std::env::consts::ARCH)
}
