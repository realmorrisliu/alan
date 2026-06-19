use crate::ShellCoreErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Domain covered by a Swift-exported parity fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureKind {
    /// Split tree and pane layout behavior.
    SplitTree,
    /// Workspace manifest decode, upgrade, pruning, or materialization.
    Manifest,
    /// Workspace state reducer behavior.
    Reducer,
    /// Action registry target and availability behavior.
    ActionRegistry,
    /// Shell control command validation and response projection.
    ControlCommand,
    /// Terminal Profile validation and launch-intent behavior.
    TerminalProfile,
    /// Reusable settings summary behavior.
    SettingsSummary,
}

/// System that produced the fixture expectation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureSource {
    /// Fixture exported from the current Swift implementation.
    Swift,
}

/// A single parity fixture case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FixtureCase {
    /// Stable id. Must match the fixture path without the `.json` extension.
    pub id: String,
    /// Domain covered by this fixture.
    pub kind: FixtureKind,
    /// Source implementation that produced the expected output.
    pub source: FixtureSource,
    /// Short human-readable description.
    pub description: String,
    /// Input domain state or manifest.
    #[serde(default)]
    pub input: Value,
    /// Operation or decode/materialization request applied to the input.
    #[serde(default)]
    pub operation: Value,
    /// Expected semantic output after normalization.
    #[serde(default)]
    pub expected: Value,
}

impl FixtureCase {
    /// Compares actual semantic output against this fixture's expected output.
    pub fn assert_expected_matches(&self, actual: &Value) -> Result<(), FixtureError> {
        if &self.expected == actual {
            return Ok(());
        }

        Err(FixtureError::ExpectedMismatch {
            id: self.id.clone(),
            expected: self.expected.clone(),
            actual: actual.clone(),
        })
    }
}

/// Loaded parity fixture corpus keyed by stable fixture id.
#[derive(Debug, Clone, Default)]
pub struct FixtureCorpus {
    cases: BTreeMap<String, FixtureCase>,
}

impl FixtureCorpus {
    /// Loads every `.json` fixture under `root`.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, FixtureError> {
        let root = root.as_ref();
        let mut cases = BTreeMap::new();
        collect_fixture_files(root, root, &mut cases)?;
        Ok(Self { cases })
    }

    /// Looks up a fixture case by stable id.
    pub fn case(&self, id: &str) -> Option<&FixtureCase> {
        self.cases.get(id)
    }
}

/// Fixture loading and validation failure.
#[derive(Debug, Error)]
pub enum FixtureError {
    /// Fixture root or case file could not be read.
    #[error("failed to read fixture path {path}: {source}")]
    Read {
        /// Path that failed.
        path: PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// Fixture file could not be decoded.
    #[error("failed to decode fixture {path}: {source}")]
    Decode {
        /// Path that failed.
        path: PathBuf,
        /// Underlying JSON error.
        source: serde_json::Error,
    },
    /// Fixture file path and embedded stable id disagree.
    #[error("fixture id mismatch for {path}: expected fixture id {expected}, found {actual}")]
    IdMismatch {
        /// Path that failed.
        path: PathBuf,
        /// Stable id derived from the file path.
        expected: String,
        /// Stable id read from the fixture payload.
        actual: String,
    },
    /// Actual semantic output did not match the fixture expectation.
    #[error(
        "fixture expected semantic output mismatch for {id}: expected {expected}, actual {actual}"
    )]
    ExpectedMismatch {
        /// Fixture id.
        id: String,
        /// Expected semantic output.
        expected: Value,
        /// Actual semantic output.
        actual: Value,
    },
}

impl FixtureError {
    /// Returns the stable shell-core error code for adapter projections.
    pub fn code(&self) -> ShellCoreErrorCode {
        match self {
            FixtureError::ExpectedMismatch { .. } => ShellCoreErrorCode::FixtureMismatch,
            FixtureError::Read { .. }
            | FixtureError::Decode { .. }
            | FixtureError::IdMismatch { .. } => ShellCoreErrorCode::InvalidFixture,
        }
    }
}

fn collect_fixture_files(
    root: &Path,
    dir: &Path,
    cases: &mut BTreeMap<String, FixtureCase>,
) -> Result<(), FixtureError> {
    let entries = fs::read_dir(dir).map_err(|source| FixtureError::Read {
        path: dir.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| FixtureError::Read {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();

        if path.is_dir() {
            collect_fixture_files(root, &path, cases)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }

        let bytes = fs::read(&path).map_err(|source| FixtureError::Read {
            path: path.clone(),
            source,
        })?;
        let case: FixtureCase =
            serde_json::from_slice(&bytes).map_err(|source| FixtureError::Decode {
                path: path.clone(),
                source,
            })?;
        let expected_id = fixture_id_for_path(root, &path);
        if case.id != expected_id {
            return Err(FixtureError::IdMismatch {
                path,
                expected: expected_id,
                actual: case.id,
            });
        }
        cases.insert(case.id.clone(), case);
    }

    Ok(())
}

fn fixture_id_for_path(root: &Path, path: &Path) -> String {
    let relative = path
        .strip_prefix(root)
        .expect("fixture path is collected from the root");
    let mut without_extension = relative.to_path_buf();
    without_extension.set_extension("");
    without_extension
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}
