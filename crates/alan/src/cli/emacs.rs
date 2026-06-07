//! `alan emacs` manages Alan-owned Emacs distribution install state.

use anyhow::{Context, Result, bail, ensure};
use clap::Subcommand;
use std::{
    env, fs, io,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

/// Alan-owned Emacs distribution commands.
#[derive(Subcommand, Clone, Copy)]
pub enum EmacsAction {
    /// Show Alan Emacs install state
    Status,
    /// Install Alan Emacs as the active Emacs config
    Install,
    /// Run deeper Alan Emacs health checks
    Doctor,
    /// Remove Alan-owned Emacs install state
    Uninstall,
}

/// Run an `alan emacs` action.
pub fn run_emacs(action: EmacsAction) -> Result<()> {
    let manager = EmacsManager::discover()?;
    match action {
        EmacsAction::Status => manager.print_status(),
        EmacsAction::Install => manager.install(),
        EmacsAction::Doctor => manager.doctor(),
        EmacsAction::Uninstall => manager.uninstall(),
    }
}

struct EmacsManager<P: EmacsProbe> {
    paths: EmacsPaths,
    source: SourceDiscovery,
    probe: P,
}

impl EmacsManager<CommandEmacsProbe> {
    fn discover() -> Result<Self> {
        Ok(Self {
            paths: EmacsPaths::from_environment()?,
            source: SourceDiscovery::discover(),
            probe: CommandEmacsProbe,
        })
    }
}

impl<P: EmacsProbe> EmacsManager<P> {
    fn print_status(&self) -> Result<()> {
        match &self.source {
            SourceDiscovery::Available(source) => println!(
                "source:  {} ({})",
                source.path.display(),
                source.kind.label()
            ),
            SourceDiscovery::Unavailable(reason) => println!("source:  unavailable ({reason})"),
        }
        println!(
            "install: {} ({})",
            self.paths.current_dir.display(),
            self.installed_copy_state().description()
        );

        let candidates = self.config_candidates()?;
        for candidate in &candidates {
            println!(
                "config:  {} -> {} ({})",
                candidate.label,
                candidate.path.display(),
                candidate.state.description()
            );
        }

        match self.select_config_entry(&candidates) {
            Ok(selection) => println!(
                "selected: {} ({})",
                selection.candidate.path.display(),
                selection.reason.description()
            ),
            Err(err) => println!("selected: unresolved ({err})"),
        }

        Ok(())
    }

    fn install(&self) -> Result<()> {
        let candidates = self.config_candidates()?;
        let selection = self.select_config_entry(&candidates)?;
        let version = self
            .probe
            .emacs_version_line()
            .context("Cannot run emacs. Install Emacs before running `alan emacs install`.")?;
        self.ensure_bare_emacs_default_matches(&selection)?;

        self.materialize_distribution()?;
        self.link_selected_config_entry(&selection)?;
        self.remove_legacy_source_links_except(&selection)?;
        self.verify_bare_startup_loads(&selection.candidate.path)?;

        println!("emacs:  {version}");
        println!("install: {}", self.paths.current_dir.display());
        println!(
            "config:  {} -> {}",
            selection.candidate.path.display(),
            self.paths.current_dir.display()
        );
        println!("bare:   default Emacs config entry matches Alan Emacs");
        Ok(())
    }

    fn doctor(&self) -> Result<()> {
        self.print_status()?;

        let mut ok = true;
        match self.probe.emacs_version_line() {
            Ok(version) => println!("check:   emacs available ({version})"),
            Err(err) => {
                ok = false;
                println!("check:   emacs unavailable ({err})");
            }
        }

        let candidates = self.config_candidates()?;
        let mut startup_expected_config = None;
        match self.select_config_entry(&candidates) {
            Ok(selection) => match self.ensure_bare_emacs_default_matches(&selection) {
                Ok(default_dir) => {
                    println!(
                        "check:   bare emacs default entry is {}",
                        default_dir.display()
                    );
                    startup_expected_config = Some(selection.candidate.path);
                }
                Err(err) => {
                    ok = false;
                    println!("check:   bare emacs default mismatch ({err})");
                }
            },
            Err(err) => {
                ok = false;
                println!("check:   config selection failed ({err})");
            }
        }

        match self.installed_copy_state() {
            InstalledCopyState::Ready => println!("check:   installed copy integrity ok"),
            state => {
                ok = false;
                println!(
                    "check:   installed copy integrity failed ({})",
                    state.description()
                );
            }
        }

        match startup_expected_config.as_deref() {
            Some(expected_config) => match self.verify_bare_startup_loads(expected_config) {
                Ok(()) => println!("check:   bare Emacs startup loads Alan Emacs"),
                Err(err) => {
                    ok = false;
                    println!("check:   bare Emacs startup failed ({err})");
                }
            },
            None => {
                println!("check:   bare Emacs startup skipped (config selection unresolved)");
            }
        }

        match self.probe.daemon_observation() {
            DaemonObservation::Connected { raw, alan_loaded } => {
                if alan_loaded {
                    println!("daemon:  connected and Alan Emacs appears loaded ({raw})");
                } else {
                    println!("daemon:  connected but Alan Emacs was not observed ({raw})");
                }
            }
            DaemonObservation::Unavailable { reason } => {
                println!("daemon:  not observed ({reason})");
            }
        }

        ensure!(ok, "Alan Emacs doctor found issues");
        println!("doctor: ok");
        Ok(())
    }

    fn uninstall(&self) -> Result<()> {
        let candidates = self.config_candidates()?;
        let alan_owned: Vec<_> = candidates
            .iter()
            .filter(|candidate| candidate.state.is_removable_alan_owned())
            .collect();

        if alan_owned.is_empty() {
            let user_owned = user_owned_candidates(&candidates);
            ensure!(
                user_owned.is_empty(),
                "Refusing to remove non-Alan-owned Emacs config: {}",
                format_candidate_paths(&user_owned)
            );
            self.remove_managed_install_data()?;
            println!("config:  no Alan-owned Emacs config entry found");
            println!("install: removed {}", self.paths.managed_root.display());
            return Ok(());
        }

        for candidate in alan_owned {
            fs::remove_file(&candidate.path)
                .with_context(|| format!("Cannot remove {}", candidate.path.display()))?;
            println!("config:  removed {}", candidate.path.display());
        }
        self.remove_managed_install_data()?;
        println!("install: removed {}", self.paths.managed_root.display());
        Ok(())
    }

    fn config_candidates(&self) -> Result<Vec<ConfigCandidate>> {
        let mut candidates = Vec::new();
        candidates.push(ConfigCandidate::inspect(
            ".emacs.el",
            self.paths.home_dir.join(".emacs.el"),
            ConfigCandidateKind::StartupFile,
            &self.paths,
            self.source.as_available(),
        )?);
        candidates.push(ConfigCandidate::inspect(
            ".emacs",
            self.paths.home_dir.join(".emacs"),
            ConfigCandidateKind::StartupFile,
            &self.paths,
            self.source.as_available(),
        )?);
        let emacs_d = self.paths.home_dir.join(".emacs.d");
        candidates.push(ConfigCandidate::inspect(
            ".emacs.d",
            emacs_d,
            ConfigCandidateKind::ConfigDirectory,
            &self.paths,
            self.source.as_available(),
        )?);

        let xdg = self.paths.config_home.join("emacs");
        if !path_eq(&xdg, &self.paths.home_dir.join(".emacs.d")) {
            candidates.push(ConfigCandidate::inspect(
                "xdg-config",
                xdg,
                ConfigCandidateKind::ConfigDirectory,
                &self.paths,
                self.source.as_available(),
            )?);
        }

        Ok(candidates)
    }

    fn select_config_entry(&self, candidates: &[ConfigCandidate]) -> Result<ConfigSelection> {
        let managed: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                candidate.kind == ConfigCandidateKind::ConfigDirectory
                    && matches!(candidate.state, ConfigEntryState::ManagedLink { .. })
            })
            .collect();
        ensure!(
            managed.len() <= 1,
            "Multiple Alan-managed Emacs config entries found: {}",
            format_candidate_paths(&managed)
        );

        let user_owned = user_owned_candidates(candidates);
        ensure!(
            user_owned.is_empty(),
            "Refusing to overwrite existing Emacs config: {}",
            format_candidate_paths(&user_owned)
        );

        if let Some(candidate) = managed.first() {
            return Ok(ConfigSelection {
                candidate: (*candidate).clone(),
                reason: SelectionReason::ExistingManagedLink,
            });
        }

        let empty: Vec<_> = candidates
            .iter()
            .filter(|candidate| {
                candidate.kind == ConfigCandidateKind::ConfigDirectory
                    && matches!(candidate.state, ConfigEntryState::EmptyDirectory)
            })
            .collect();
        ensure!(
            empty.len() <= 1,
            "Multiple empty Emacs config candidates found: {}",
            format_candidate_paths(&empty)
        );
        if let Some(candidate) = empty.first() {
            return Ok(ConfigSelection {
                candidate: (*candidate).clone(),
                reason: SelectionReason::SingleEmptyCandidate,
            });
        }

        let default_dir = self
            .probe
            .default_user_emacs_directory()
            .context("Cannot determine Emacs default user config directory")?;
        let Some(candidate) = candidates.iter().find(|candidate| {
            candidate.kind == ConfigCandidateKind::ConfigDirectory
                && path_eq(&candidate.path, &default_dir)
        }) else {
            bail!(
                "Emacs default config directory is unsupported: {}",
                default_dir.display()
            );
        };

        ensure!(
            matches!(
                candidate.state,
                ConfigEntryState::Missing | ConfigEntryState::LegacySourceLink { .. }
            ),
            "Emacs default config entry is not installable: {} ({})",
            candidate.path.display(),
            candidate.state.description()
        );

        Ok(ConfigSelection {
            candidate: candidate.clone(),
            reason: SelectionReason::EmacsDefaultProbe(default_dir),
        })
    }

    fn ensure_bare_emacs_default_matches(&self, selection: &ConfigSelection) -> Result<PathBuf> {
        let default_dir = self
            .probe
            .default_user_emacs_directory()
            .context("Cannot determine Emacs default user config directory")?;
        ensure!(
            path_eq(&default_dir, &selection.candidate.path),
            "bare emacs uses {}, but Alan Emacs would be installed at {}",
            default_dir.display(),
            selection.candidate.path.display()
        );
        Ok(default_dir)
    }

    fn materialize_distribution(&self) -> Result<()> {
        let source = self.required_source()?;
        ensure_distribution_dir(&source.path)?;
        fs::create_dir_all(&self.paths.managed_root)
            .with_context(|| format!("Cannot create {}", self.paths.managed_root.display()))?;

        let temp_dir = self
            .paths
            .managed_root
            .join(format!("current.tmp-{}", std::process::id()));
        remove_path_if_exists(&temp_dir)
            .with_context(|| format!("Cannot clear {}", temp_dir.display()))?;
        copy_dir_recursive(&source.path, &temp_dir)
            .with_context(|| format!("Cannot copy {}", source.path.display()))?;
        ensure_distribution_dir(&temp_dir)?;

        remove_path_if_exists(&self.paths.current_dir)
            .with_context(|| format!("Cannot replace {}", self.paths.current_dir.display()))?;
        fs::rename(&temp_dir, &self.paths.current_dir).with_context(|| {
            format!(
                "Cannot install {} to {}",
                temp_dir.display(),
                self.paths.current_dir.display()
            )
        })?;

        if let (Ok(source), Ok(current)) = (
            fs::canonicalize(&source.path),
            fs::canonicalize(&self.paths.current_dir),
        ) {
            ensure!(
                source != current,
                "Installed Alan Emacs copy must not point at the source checkout"
            );
        }
        Ok(())
    }

    fn link_selected_config_entry(&self, selection: &ConfigSelection) -> Result<()> {
        let path = &selection.candidate.path;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Cannot create {}", parent.display()))?;
        }

        match selection.candidate.state {
            ConfigEntryState::Missing => {}
            ConfigEntryState::EmptyDirectory => {
                fs::remove_dir(path)
                    .with_context(|| format!("Cannot remove empty directory {}", path.display()))?;
            }
            ConfigEntryState::ManagedLink { .. } | ConfigEntryState::LegacySourceLink { .. } => {
                fs::remove_file(path)
                    .with_context(|| format!("Cannot replace {}", path.display()))?;
            }
            _ => bail!(
                "Refusing to replace non-Alan-owned Emacs config: {}",
                path.display()
            ),
        }

        symlink_dir(&self.paths.current_dir, path)?;
        Ok(())
    }

    fn remove_legacy_source_links_except(&self, selection: &ConfigSelection) -> Result<()> {
        for candidate in self.config_candidates()? {
            if path_eq(&candidate.path, &selection.candidate.path) {
                continue;
            }
            if matches!(candidate.state, ConfigEntryState::LegacySourceLink { .. }) {
                fs::remove_file(&candidate.path)
                    .with_context(|| format!("Cannot remove {}", candidate.path.display()))?;
            }
        }
        Ok(())
    }

    fn verify_bare_startup_loads(&self, expected_config_dir: &Path) -> Result<()> {
        let check = self
            .probe
            .verify_bare_startup_loads()
            .context("Cannot verify bare Emacs startup")?;
        ensure!(check.loaded, "Alan Emacs load marker was not observed");
        let Some(user_dir) = check.user_emacs_directory else {
            bail!("Alan Emacs did not report user-emacs-directory");
        };
        ensure!(
            path_eq(&user_dir, expected_config_dir),
            "bare Emacs startup used {}, expected {}",
            user_dir.display(),
            expected_config_dir.display()
        );
        let Some(resolved_dir) = check.resolved_user_emacs_directory else {
            bail!("Alan Emacs did not report resolved user-emacs-directory");
        };
        ensure!(
            path_eq(&resolved_dir, &self.paths.current_dir),
            "bare Emacs startup resolved to {}, expected {}",
            resolved_dir.display(),
            self.paths.current_dir.display()
        );
        Ok(())
    }

    fn installed_copy_state(&self) -> InstalledCopyState {
        let metadata = match fs::symlink_metadata(&self.paths.current_dir) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return InstalledCopyState::Missing;
            }
            Err(err) => return InstalledCopyState::Unreadable(err.to_string()),
        };
        if metadata.file_type().is_symlink() {
            return InstalledCopyState::UnexpectedSymlink;
        }
        if !metadata.is_dir() {
            return InstalledCopyState::NotDirectory;
        }

        let required = ["init.el", "early-init.el", "lisp/alan-core.el"];
        let missing: Vec<_> = required
            .iter()
            .filter(|relative| !self.paths.current_dir.join(relative).is_file())
            .map(|relative| (*relative).to_string())
            .collect();
        if missing.is_empty() {
            InstalledCopyState::Ready
        } else {
            InstalledCopyState::Incomplete(missing)
        }
    }

    fn remove_managed_install_data(&self) -> Result<()> {
        ensure!(
            self.paths.managed_root.ends_with("alan/emacs"),
            "Refusing to remove unexpected Alan Emacs install root: {}",
            self.paths.managed_root.display()
        );
        remove_path_if_exists(&self.paths.managed_root)
    }

    fn required_source(&self) -> Result<&DistributionSource> {
        match &self.source {
            SourceDiscovery::Available(source) => Ok(source),
            SourceDiscovery::Unavailable(reason) => {
                bail!("Cannot find Alan Emacs distribution resource: {reason}")
            }
        }
    }
}

#[derive(Clone, Debug)]
struct EmacsPaths {
    home_dir: PathBuf,
    config_home: PathBuf,
    managed_root: PathBuf,
    current_dir: PathBuf,
}

impl EmacsPaths {
    fn from_environment() -> Result<Self> {
        let home_dir = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(dirs::home_dir)
            .context("Cannot determine home directory")?;
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir.join(".config"));
        let data_home = env::var_os("XDG_DATA_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir.join(".local/share"));
        Ok(Self::new(home_dir, config_home, data_home))
    }

    fn new(home_dir: PathBuf, config_home: PathBuf, data_home: PathBuf) -> Self {
        let home_dir = normalize_lexical(&home_dir);
        let config_home = normalize_lexical(&config_home);
        let data_home = normalize_lexical(&data_home);
        let managed_root = data_home.join("alan/emacs");
        let current_dir = managed_root.join("current");
        Self {
            home_dir,
            config_home,
            managed_root,
            current_dir,
        }
    }
}

#[derive(Clone, Debug)]
struct DistributionSource {
    kind: DistributionSourceKind,
    path: PathBuf,
}

#[derive(Clone, Debug)]
enum SourceDiscovery {
    Available(DistributionSource),
    Unavailable(String),
}

impl SourceDiscovery {
    fn discover() -> Self {
        match DistributionSource::discover() {
            Ok(source) => Self::Available(source),
            Err(err) => Self::Unavailable(err.to_string()),
        }
    }

    fn as_available(&self) -> Option<&DistributionSource> {
        match self {
            Self::Available(source) => Some(source),
            Self::Unavailable(_) => None,
        }
    }
}

impl DistributionSource {
    fn discover() -> Result<Self> {
        let mut candidates = Vec::new();

        if let Some(path) = env::var_os("ALAN_EMACS_RESOURCE_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
        {
            candidates.push((DistributionSourceKind::EnvironmentOverride, path));
        }

        if let Ok(executable) = env::current_exe() {
            for resource in bundled_resource_candidates_from_executable(&executable) {
                candidates.push((DistributionSourceKind::BundledResource, resource));
            }
        }

        candidates.push((
            DistributionSourceKind::DevelopmentSource,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/alan-emacs"),
        ));

        if let Ok(current_dir) = env::current_dir() {
            for ancestor in current_dir.ancestors() {
                candidates.push((
                    DistributionSourceKind::DevelopmentSource,
                    ancestor.join("tools/alan-emacs"),
                ));
            }
        }

        for (kind, path) in candidates {
            let path = normalize_lexical(&path);
            if ensure_distribution_dir(&path).is_ok() {
                return Ok(Self { kind, path });
            }
        }

        bail!(
            "Cannot find Alan Emacs distribution resource. Expected bundled Resources/alan-emacs or tools/alan-emacs in a development checkout."
        )
    }
}

#[derive(Clone, Debug)]
enum DistributionSourceKind {
    EnvironmentOverride,
    BundledResource,
    DevelopmentSource,
}

impl DistributionSourceKind {
    fn label(&self) -> &'static str {
        match self {
            Self::EnvironmentOverride => "environment override",
            Self::BundledResource => "bundled resource",
            Self::DevelopmentSource => "development source",
        }
    }
}

fn bundled_resource_candidates_from_executable(executable: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for executable in executable_and_resolved_targets(executable) {
        if let Some(resources) = executable
            .parent()
            .and_then(Path::parent)
            .filter(|path| path.ends_with("Resources"))
        {
            push_unique_path(&mut candidates, resources.join("alan-emacs"));
        }
    }
    candidates
}

fn executable_and_resolved_targets(executable: &Path) -> Vec<PathBuf> {
    let mut paths = vec![normalize_lexical(executable)];
    if let Ok(resolved) = fs::canonicalize(executable) {
        push_unique_path(&mut paths, normalize_lexical(&resolved));
    }
    paths
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| path_eq(existing, &path)) {
        paths.push(path);
    }
}

#[derive(Clone, Debug)]
struct ConfigCandidate {
    label: &'static str,
    path: PathBuf,
    kind: ConfigCandidateKind,
    state: ConfigEntryState,
}

impl ConfigCandidate {
    fn inspect(
        label: &'static str,
        path: PathBuf,
        kind: ConfigCandidateKind,
        paths: &EmacsPaths,
        source: Option<&DistributionSource>,
    ) -> Result<Self> {
        let path = normalize_lexical(&path);
        let state = ConfigEntryState::inspect(&path, kind, paths, source)?;
        Ok(Self {
            label,
            path,
            kind,
            state,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfigCandidateKind {
    ConfigDirectory,
    StartupFile,
}

#[derive(Clone, Debug)]
enum ConfigEntryState {
    Missing,
    EmptyDirectory,
    ManagedLink { target: PathBuf },
    LegacySourceLink { target: PathBuf },
    StartupFileConflict { target: Option<PathBuf> },
    UserOwnedSymlink { target: PathBuf },
    UserOwnedDirectory,
    UserOwnedFile,
    Unreadable(String),
}

impl ConfigEntryState {
    fn inspect(
        path: &Path,
        kind: ConfigCandidateKind,
        paths: &EmacsPaths,
        source: Option<&DistributionSource>,
    ) -> Result<Self> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::Missing),
            Err(err) => return Ok(Self::Unreadable(err.to_string())),
        };

        if kind == ConfigCandidateKind::StartupFile {
            let target = if metadata.file_type().is_symlink() {
                Some(resolve_symlink_target(
                    path,
                    fs::read_link(path)
                        .with_context(|| format!("Cannot read symlink {}", path.display()))?,
                ))
            } else {
                None
            };
            return Ok(Self::StartupFileConflict { target });
        }

        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path)
                .with_context(|| format!("Cannot read symlink {}", path.display()))?;
            let target = resolve_symlink_target(path, target);
            if path_within(&target, &paths.managed_root) {
                return Ok(Self::ManagedLink { target });
            }
            if is_legacy_source_link_target(&target, source) {
                return Ok(Self::LegacySourceLink { target });
            }
            return Ok(Self::UserOwnedSymlink { target });
        }

        if metadata.is_dir() {
            match fs::read_dir(path) {
                Ok(mut entries) => match entries.next() {
                    None => Ok(Self::EmptyDirectory),
                    Some(Ok(_)) => Ok(Self::UserOwnedDirectory),
                    Some(Err(err)) => Ok(Self::Unreadable(err.to_string())),
                },
                Err(err) => Ok(Self::Unreadable(err.to_string())),
            }
        } else {
            Ok(Self::UserOwnedFile)
        }
    }

    fn is_user_owned(&self) -> bool {
        matches!(
            self,
            Self::StartupFileConflict { .. }
                | Self::UserOwnedSymlink { .. }
                | Self::UserOwnedDirectory
                | Self::UserOwnedFile
                | Self::Unreadable(_)
        )
    }

    fn is_removable_alan_owned(&self) -> bool {
        matches!(
            self,
            Self::ManagedLink { .. } | Self::LegacySourceLink { .. }
        )
    }

    fn description(&self) -> String {
        match self {
            Self::Missing => "missing".to_string(),
            Self::EmptyDirectory => "empty directory".to_string(),
            Self::ManagedLink { target } => {
                format!("Alan-managed link to {}", target.display())
            }
            Self::LegacySourceLink { target } => {
                format!("legacy Alan source link to {}", target.display())
            }
            Self::StartupFileConflict { target } => {
                if let Some(target) = target {
                    format!(
                        "startup file shadows Emacs config directory; symlink to {}",
                        target.display()
                    )
                } else {
                    "startup file shadows Emacs config directory".to_string()
                }
            }
            Self::UserOwnedSymlink { target } => {
                format!("non-Alan symlink to {}", target.display())
            }
            Self::UserOwnedDirectory => "non-empty non-Alan directory".to_string(),
            Self::UserOwnedFile => "non-Alan file".to_string(),
            Self::Unreadable(reason) => format!("unreadable: {reason}"),
        }
    }
}

#[derive(Clone, Debug)]
struct ConfigSelection {
    candidate: ConfigCandidate,
    reason: SelectionReason,
}

#[derive(Clone, Debug)]
enum SelectionReason {
    ExistingManagedLink,
    SingleEmptyCandidate,
    EmacsDefaultProbe(PathBuf),
}

impl SelectionReason {
    fn description(&self) -> String {
        match self {
            Self::ExistingManagedLink => "existing Alan-managed entry".to_string(),
            Self::SingleEmptyCandidate => "single empty candidate".to_string(),
            Self::EmacsDefaultProbe(path) => {
                format!("Emacs default probe: {}", path.display())
            }
        }
    }
}

enum InstalledCopyState {
    Ready,
    Missing,
    UnexpectedSymlink,
    NotDirectory,
    Incomplete(Vec<String>),
    Unreadable(String),
}

impl InstalledCopyState {
    fn description(&self) -> String {
        match self {
            Self::Ready => "ready".to_string(),
            Self::Missing => "missing".to_string(),
            Self::UnexpectedSymlink => "unexpected symlink".to_string(),
            Self::NotDirectory => "not a directory".to_string(),
            Self::Incomplete(missing) => format!("missing {}", missing.join(", ")),
            Self::Unreadable(reason) => format!("unreadable: {reason}"),
        }
    }
}

trait EmacsProbe {
    fn emacs_version_line(&self) -> Result<String>;
    fn default_user_emacs_directory(&self) -> Result<PathBuf>;
    fn verify_bare_startup_loads(&self) -> Result<LoadCheck>;
    fn daemon_observation(&self) -> DaemonObservation;
}

struct CommandEmacsProbe;

impl CommandEmacsProbe {
    fn output(program: &str, args: &[&str]) -> Result<std::process::Output> {
        Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("Cannot run {program}"))
    }
}

impl EmacsProbe for CommandEmacsProbe {
    fn emacs_version_line(&self) -> Result<String> {
        let output = Self::output("emacs", &["--version"])?;
        ensure!(
            output.status.success(),
            "emacs --version failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.lines().next().unwrap_or("emacs").to_string())
    }

    fn default_user_emacs_directory(&self) -> Result<PathBuf> {
        let output = Self::output(
            "emacs",
            &[
                "--batch",
                "--eval",
                "(princ (expand-file-name user-emacs-directory))",
            ],
        )?;
        ensure!(
            output.status.success(),
            "emacs default-directory probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        ensure!(
            !stdout.is_empty(),
            "emacs default-directory probe returned empty output"
        );
        Ok(normalize_lexical(Path::new(&stdout)))
    }

    fn verify_bare_startup_loads(&self) -> Result<LoadCheck> {
        let nonce = startup_probe_nonce();
        let marker_path = env::temp_dir().join(format!("alan-emacs-startup-{nonce}.txt"));
        let _ = fs::remove_file(&marker_path);

        let marker_literal = emacs_lisp_string_literal(&marker_path.to_string_lossy());
        let eval = format!(
            "(progn \
             (with-temp-file {marker_literal} \
               (insert (if (and (boundp 'alan-emacs-loaded) alan-emacs-loaded) \
                           (concat (expand-file-name user-emacs-directory) \
                                   \"\\n\" \
                                   (file-truename user-emacs-directory)) \
                           \"\"))) \
             (kill-emacs 0))"
        );
        let daemon_arg = format!("--fg-daemon=alan-emacs-probe-{nonce}");
        let output = Command::new("emacs")
            .args([daemon_arg.as_str(), "--eval", eval.as_str()])
            .output()
            .context("Cannot run bare Emacs startup verification")?;

        let marker = match fs::read_to_string(&marker_path) {
            Ok(marker) => marker,
            Err(err) if err.kind() == io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Err(err).with_context(|| format!("Cannot read {}", marker_path.display()));
            }
        };
        let _ = fs::remove_file(&marker_path);

        ensure!(
            output.status.success(),
            "bare Emacs startup verification failed: {}",
            command_output_summary(&output)
        );
        let mut marker_lines = marker.lines();
        let user_emacs_directory = marker_lines
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| normalize_lexical(Path::new(value)));
        let resolved_user_emacs_directory = marker_lines
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| normalize_lexical(Path::new(value)));
        let loaded = user_emacs_directory.is_some();
        Ok(LoadCheck {
            loaded,
            user_emacs_directory,
            resolved_user_emacs_directory,
        })
    }

    fn daemon_observation(&self) -> DaemonObservation {
        let output = match Self::output(
            "emacsclient",
            &[
                "-e",
                "(list :daemon (daemonp) :server-name server-name :user-emacs-directory (expand-file-name user-emacs-directory) :alan-emacs-loaded (and (boundp 'alan-emacs-loaded) alan-emacs-loaded))",
            ],
        ) {
            Ok(output) => output,
            Err(err) => {
                return DaemonObservation::Unavailable {
                    reason: err.to_string(),
                };
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return DaemonObservation::Unavailable {
                reason: if stderr.is_empty() {
                    "emacsclient could not connect".to_string()
                } else {
                    stderr
                },
            };
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let alan_loaded = raw.contains(":alan-emacs-loaded t");
        DaemonObservation::Connected { raw, alan_loaded }
    }
}

struct LoadCheck {
    loaded: bool,
    user_emacs_directory: Option<PathBuf>,
    resolved_user_emacs_directory: Option<PathBuf>,
}

enum DaemonObservation {
    Connected { raw: String, alan_loaded: bool },
    Unavailable { reason: String },
}

fn ensure_distribution_dir(path: &Path) -> Result<()> {
    ensure!(
        path.join("init.el").is_file(),
        "Alan Emacs distribution is missing init.el: {}",
        path.display()
    );
    ensure!(
        path.join("early-init.el").is_file(),
        "Alan Emacs distribution is missing early-init.el: {}",
        path.display()
    );
    ensure!(
        path.join("lisp/alan-core.el").is_file(),
        "Alan Emacs distribution is missing lisp/alan-core.el: {}",
        path.display()
    );
    Ok(())
}

fn is_legacy_source_link_target(target: &Path, source: Option<&DistributionSource>) -> bool {
    if source.is_some_and(|source| paths_equal_existing_or_lexical(target, &source.path)) {
        return true;
    }
    if ensure_distribution_dir(target).is_ok() {
        return true;
    }
    matches!(target.try_exists(), Ok(false)) && target.ends_with(Path::new("tools/alan-emacs"))
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)
        .with_context(|| format!("Cannot create {}", destination.display()))?;
    for entry in
        fs::read_dir(source).with_context(|| format!("Cannot read {}", source.display()))?
    {
        let entry = entry?;
        let file_name = entry.file_name();
        if should_skip_distribution_entry(&file_name) {
            continue;
        }
        let source_path = entry.path();
        let destination_path = destination.join(&file_name);
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&source_path)
                .with_context(|| format!("Cannot read symlink {}", source_path.display()))?;
            symlink_dir_or_file(&target, &destination_path)?;
        } else if metadata.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path).with_context(|| {
                format!(
                    "Cannot copy {} to {}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
            fs::set_permissions(&destination_path, metadata.permissions()).with_context(|| {
                format!("Cannot set permissions on {}", destination_path.display())
            })?;
        }
    }
    Ok(())
}

fn should_skip_distribution_entry(file_name: &std::ffi::OsStr) -> bool {
    matches!(
        file_name.to_str(),
        Some(".git" | ".DS_Store" | "target" | "var" | "eln-cache")
    )
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("Cannot inspect {}", path.display())),
    };

    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .with_context(|| format!("Cannot remove {}", path.display()))
}

fn user_owned_candidates(candidates: &[ConfigCandidate]) -> Vec<&ConfigCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.state.is_user_owned())
        .collect()
}

fn format_candidate_paths(candidates: &[&ConfigCandidate]) -> String {
    candidates
        .iter()
        .map(|candidate| {
            format!(
                "{} ({})",
                candidate.path.display(),
                candidate.state.description()
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn startup_probe_nonce() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{timestamp}", std::process::id())
}

fn emacs_lisp_string_literal(value: &str) -> String {
    let mut literal = String::with_capacity(value.len() + 2);
    literal.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            '\t' => literal.push_str("\\t"),
            _ => literal.push(ch),
        }
    }
    literal.push('"');
    literal
}

fn command_output_summary(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        format!("emacs exited with {}", output.status)
    } else {
        stdout
    }
}

fn resolve_symlink_target(link_path: &Path, target: PathBuf) -> PathBuf {
    if target.is_absolute() {
        normalize_lexical(&target)
    } else {
        normalize_lexical(
            &link_path
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .join(target),
        )
    }
}

fn paths_equal_existing_or_lexical(lhs: &Path, rhs: &Path) -> bool {
    if let (Ok(lhs), Ok(rhs)) = (fs::canonicalize(lhs), fs::canonicalize(rhs)) {
        return lhs == rhs;
    }
    path_eq(lhs, rhs)
}

fn path_within(path: &Path, root: &Path) -> bool {
    let path = normalize_lexical(path);
    let root = normalize_lexical(root);
    path == root || path.starts_with(root)
}

fn path_eq(lhs: &Path, rhs: &Path) -> bool {
    normalize_lexical(lhs) == normalize_lexical(rhs)
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("Cannot link {} -> {}", link.display(), target.display()))
}

#[cfg(not(unix))]
fn symlink_dir(_target: &Path, _link: &Path) -> Result<()> {
    bail!("Alan Emacs install currently requires Unix-style config symlinks")
}

#[cfg(unix)]
fn symlink_dir_or_file(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("Cannot link {} -> {}", link.display(), target.display()))
}

#[cfg(not(unix))]
fn symlink_dir_or_file(_target: &Path, _link: &Path) -> Result<()> {
    bail!("Alan Emacs distribution copy currently requires Unix-style symlinks")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[derive(Clone)]
    struct StaticProbe {
        version: Option<String>,
        default_dir: PathBuf,
        load_check: LoadCheckFixture,
        daemon: DaemonObservationFixture,
    }

    #[derive(Clone)]
    struct LoadCheckFixture {
        loaded: bool,
        user_emacs_directory: Option<PathBuf>,
        resolved_user_emacs_directory: Option<PathBuf>,
    }

    #[derive(Clone)]
    enum DaemonObservationFixture {
        Connected { raw: String, alan_loaded: bool },
        Unavailable { reason: String },
    }

    impl EmacsProbe for StaticProbe {
        fn emacs_version_line(&self) -> Result<String> {
            self.version
                .clone()
                .context("emacs unavailable in test probe")
        }

        fn default_user_emacs_directory(&self) -> Result<PathBuf> {
            Ok(self.default_dir.clone())
        }

        fn verify_bare_startup_loads(&self) -> Result<LoadCheck> {
            Ok(LoadCheck {
                loaded: self.load_check.loaded,
                user_emacs_directory: self.load_check.user_emacs_directory.clone(),
                resolved_user_emacs_directory: self
                    .load_check
                    .resolved_user_emacs_directory
                    .clone(),
            })
        }

        fn daemon_observation(&self) -> DaemonObservation {
            match &self.daemon {
                DaemonObservationFixture::Connected { raw, alan_loaded } => {
                    DaemonObservation::Connected {
                        raw: raw.clone(),
                        alan_loaded: *alan_loaded,
                    }
                }
                DaemonObservationFixture::Unavailable { reason } => {
                    DaemonObservation::Unavailable {
                        reason: reason.clone(),
                    }
                }
            }
        }
    }

    fn make_manager(
        home: &Path,
        source: PathBuf,
        default_dir: PathBuf,
    ) -> EmacsManager<StaticProbe> {
        let paths = EmacsPaths::new(
            home.to_path_buf(),
            home.join(".config"),
            home.join(".local/share"),
        );
        let current_dir = paths.current_dir.clone();
        let startup_user_dir = default_dir.clone();
        EmacsManager {
            paths,
            source: SourceDiscovery::Available(DistributionSource {
                kind: DistributionSourceKind::DevelopmentSource,
                path: source,
            }),
            probe: StaticProbe {
                version: Some("GNU Emacs 30.2".to_string()),
                default_dir,
                load_check: LoadCheckFixture {
                    loaded: true,
                    user_emacs_directory: Some(startup_user_dir),
                    resolved_user_emacs_directory: Some(current_dir),
                },
                daemon: DaemonObservationFixture::Unavailable {
                    reason: "not running".to_string(),
                },
            },
        }
    }

    fn make_source(root: &Path) -> PathBuf {
        let source = root.join("tools/alan-emacs");
        fs::create_dir_all(source.join("lisp")).unwrap();
        fs::write(
            source.join("init.el"),
            "(setq user-emacs-directory (file-name-directory (or load-file-name buffer-file-name)))\n(setq alan-emacs-loaded t)\n",
        )
        .unwrap();
        fs::write(
            source.join("early-init.el"),
            "(setq package-enable-at-startup nil)\n",
        )
        .unwrap();
        fs::write(source.join("lisp/alan-core.el"), "(provide 'alan-core)\n").unwrap();
        source
    }

    #[test]
    fn emacs_lisp_string_literal_escapes_probe_marker_paths() {
        assert_eq!(
            emacs_lisp_string_literal("/tmp/alan \"probe\"\\marker\n"),
            "\"/tmp/alan \\\"probe\\\"\\\\marker\\n\""
        );
    }

    #[test]
    fn bundled_resource_candidates_follow_cli_symlink() {
        let temp = TempDir::new().unwrap();
        let resources = temp.path().join("Alan.app/Contents/Resources");
        let executable = resources.join("bin/alan");
        let command_dir = temp.path().join("usr/local/bin");
        let command_link = command_dir.join("alan");
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::create_dir_all(resources.join("alan-emacs")).unwrap();
        fs::create_dir_all(&command_dir).unwrap();
        fs::write(&executable, "").unwrap();
        symlink_dir_or_file(&executable, &command_link).unwrap();

        let candidates = bundled_resource_candidates_from_executable(&command_link);

        assert_eq!(candidates.len(), 1);
        assert!(paths_equal_existing_or_lexical(
            &candidates[0],
            &resources.join("alan-emacs")
        ));
    }

    #[test]
    fn bare_startup_check_rejects_unexpected_config_entry() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let mut manager = make_manager(&home, source, default_dir.clone());
        manager.probe.load_check.user_emacs_directory = Some(home.join(".config/emacs"));

        let err = manager.verify_bare_startup_loads(&default_dir).unwrap_err();

        assert!(err.to_string().contains("bare Emacs startup used"));
    }

    #[test]
    fn bare_startup_check_rejects_config_entry_not_resolving_to_current() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let mut manager = make_manager(&home, source, default_dir.clone());
        manager.probe.load_check.resolved_user_emacs_directory =
            Some(home.join(".local/share/alan/emacs/old"));

        let err = manager.verify_bare_startup_loads(&default_dir).unwrap_err();

        assert!(err.to_string().contains("bare Emacs startup resolved"));
    }

    #[test]
    fn selects_emacs_probe_when_candidates_are_missing() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let manager = make_manager(&home, source, default_dir.clone());

        let selection = manager
            .select_config_entry(&manager.config_candidates().unwrap())
            .unwrap();

        assert!(path_eq(&selection.candidate.path, &default_dir));
        assert!(matches!(
            selection.reason,
            SelectionReason::EmacsDefaultProbe(_)
        ));
    }

    #[test]
    fn selects_single_empty_candidate() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let empty = home.join(".config/emacs");
        fs::create_dir_all(&empty).unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let selection = manager
            .select_config_entry(&manager.config_candidates().unwrap())
            .unwrap();

        assert!(path_eq(&selection.candidate.path, &empty));
        assert!(matches!(
            selection.reason,
            SelectionReason::SingleEmptyCandidate
        ));
    }

    #[test]
    fn reuses_existing_managed_link() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".config/emacs"));
        fs::create_dir_all(&manager.paths.current_dir).unwrap();
        fs::create_dir_all(home.join(".config")).unwrap();
        symlink_dir(&manager.paths.current_dir, &home.join(".config/emacs")).unwrap();

        let selection = manager
            .select_config_entry(&manager.config_candidates().unwrap())
            .unwrap();

        assert!(path_eq(
            &selection.candidate.path,
            &home.join(".config/emacs")
        ));
        assert!(matches!(
            selection.reason,
            SelectionReason::ExistingManagedLink
        ));
    }

    #[test]
    fn refuses_user_owned_non_empty_config() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".emacs.d")).unwrap();
        fs::write(home.join(".emacs.d/init.el"), "(setq user-config t)").unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let err = manager
            .select_config_entry(&manager.config_candidates().unwrap())
            .unwrap_err();

        assert!(err.to_string().contains("Refusing to overwrite"));
    }

    #[test]
    fn install_refuses_legacy_emacs_init_file() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".emacs"), "(setq user-config t)").unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let err = manager.install().unwrap_err();

        let message = err.to_string();
        assert!(message.contains("Refusing to overwrite"));
        assert!(message.contains(".emacs"));
        assert!(!manager.paths.current_dir.exists());
    }

    #[test]
    fn install_refuses_legacy_emacs_el_init_file() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        fs::write(home.join(".emacs.el"), "(setq user-config t)").unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let err = manager.install().unwrap_err();

        let message = err.to_string();
        assert!(message.contains("Refusing to overwrite"));
        assert!(message.contains(".emacs.el"));
        assert!(!manager.paths.current_dir.exists());
    }

    #[test]
    fn refuses_multiple_user_owned_configs() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".emacs.d")).unwrap();
        fs::write(home.join(".emacs.d/init.el"), "(setq user-config t)").unwrap();
        fs::create_dir_all(home.join(".config/emacs")).unwrap();
        fs::write(home.join(".config/emacs/init.el"), "(setq other-config t)").unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let err = manager
            .select_config_entry(&manager.config_candidates().unwrap())
            .unwrap_err();

        let message = err.to_string();
        assert!(message.contains(".emacs.d"));
        assert!(message.contains(".config/emacs"));
    }

    #[test]
    fn install_copies_distribution_and_links_default_entry() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let manager = make_manager(&home, source.clone(), default_dir.clone());

        manager.install().unwrap();

        assert!(manager.paths.current_dir.join("init.el").is_file());
        assert!(
            fs::symlink_metadata(&default_dir)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let target = fs::read_link(&default_dir).unwrap();
        assert!(path_eq(&target, &manager.paths.current_dir));
        assert!(!paths_equal_existing_or_lexical(
            &source,
            &manager.paths.current_dir
        ));
    }

    #[test]
    fn install_is_idempotent_for_managed_link() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let manager = make_manager(&home, source, default_dir.clone());

        manager.install().unwrap();
        manager.install().unwrap();

        let target = fs::read_link(&default_dir).unwrap();
        assert!(path_eq(&target, &manager.paths.current_dir));
    }

    #[test]
    fn install_migrates_legacy_source_link_when_emacs_default_differs() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".config")).unwrap();
        let source = make_source(temp.path());
        symlink_dir(&source, &home.join(".config/emacs")).unwrap();
        let default_dir = home.join(".emacs.d");
        let manager = make_manager(&home, source, default_dir.clone());

        manager.install().unwrap();

        assert!(
            fs::symlink_metadata(&default_dir)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!home.join(".config/emacs").exists());
    }

    #[test]
    fn install_migrates_legacy_source_link_when_current_source_differs() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let legacy_source = make_source(&temp.path().join("legacy-checkout"));
        let current_source = make_source(&temp.path().join("current-bundle"));
        let default_dir = home.join(".emacs.d");
        symlink_dir(&legacy_source, &default_dir).unwrap();
        let manager = make_manager(&home, current_source, default_dir.clone());

        manager.install().unwrap();

        let target = fs::read_link(&default_dir).unwrap();
        assert!(path_eq(&target, &manager.paths.current_dir));
    }

    #[test]
    fn install_preserves_existing_non_alan_tools_named_symlink() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let foreign_target = temp.path().join("foreign/tools/alan-emacs");
        fs::create_dir_all(&foreign_target).unwrap();
        fs::write(foreign_target.join("init.el"), "(setq user-config t)").unwrap();
        let source = make_source(&temp.path().join("current-bundle"));
        let default_dir = home.join(".emacs.d");
        symlink_dir(&foreign_target, &default_dir).unwrap();
        let manager = make_manager(&home, source, default_dir.clone());

        let err = manager.install().unwrap_err();

        let message = err.to_string();
        assert!(message.contains("Refusing to overwrite"));
        let target = fs::read_link(&default_dir).unwrap();
        assert!(path_eq(&target, &foreign_target));
        assert!(foreign_target.join("init.el").is_file());
    }

    #[test]
    fn doctor_fails_when_startup_file_shadows_installed_config() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let manager = make_manager(&home, source, default_dir.clone());
        manager.install().unwrap();

        fs::write(home.join(".emacs"), "(setq user-config t)").unwrap();
        let selection_err = manager
            .select_config_entry(&manager.config_candidates().unwrap())
            .unwrap_err();
        let selection_message = selection_err.to_string();
        assert!(selection_message.contains(".emacs"));
        assert!(selection_message.contains("startup file shadows"));

        let err = manager.doctor().unwrap_err();

        let message = err.to_string();
        assert!(message.contains("Alan Emacs doctor found issues"));
    }

    #[test]
    fn install_refuses_when_selected_entry_is_not_bare_default() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let selected = home.join(".config/emacs");
        fs::create_dir_all(&selected).unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let err = manager.install().unwrap_err();

        assert!(err.to_string().contains("bare emacs uses"));
        assert!(selected.is_dir());
        assert!(
            !fs::symlink_metadata(&selected)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!manager.paths.current_dir.exists());
    }

    #[test]
    fn uninstall_removes_only_alan_owned_links_and_data() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let manager = make_manager(&home, source, default_dir.clone());
        manager.install().unwrap();

        fs::create_dir_all(home.join(".config/emacs")).unwrap();
        fs::write(home.join(".config/emacs/init.el"), "(setq user-config t)").unwrap();
        manager.uninstall().unwrap();

        assert!(!default_dir.exists());
        assert!(!manager.paths.managed_root.exists());
        assert!(home.join(".config/emacs/init.el").is_file());
    }

    #[test]
    fn uninstall_removes_managed_state_when_source_is_unavailable() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let mut manager = make_manager(&home, source, default_dir.clone());
        manager.install().unwrap();
        manager.source = SourceDiscovery::Unavailable("source removed".to_string());

        manager.uninstall().unwrap();

        assert!(!default_dir.exists());
        assert!(!manager.paths.managed_root.exists());
    }

    #[test]
    fn uninstall_removes_existing_legacy_source_link_when_source_is_unavailable() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let legacy_source = make_source(&temp.path().join("legacy-checkout"));
        let current_source = make_source(&temp.path().join("current-bundle"));
        let default_dir = home.join(".emacs.d");
        symlink_dir(&legacy_source, &default_dir).unwrap();
        let mut manager = make_manager(&home, current_source, default_dir.clone());
        manager.source = SourceDiscovery::Unavailable("source removed".to_string());

        manager.uninstall().unwrap();

        assert!(!default_dir.exists());
    }

    #[test]
    fn uninstall_removes_deleted_legacy_source_link_by_target_shape() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let deleted_source = temp.path().join("deleted-checkout/tools/alan-emacs");
        let current_source = make_source(&temp.path().join("current-bundle"));
        let default_dir = home.join(".emacs.d");
        symlink_dir(&deleted_source, &default_dir).unwrap();
        let mut manager = make_manager(&home, current_source, default_dir.clone());
        manager.source = SourceDiscovery::Unavailable("source removed".to_string());

        manager.uninstall().unwrap();

        assert!(fs::symlink_metadata(&default_dir).is_err());
    }

    #[test]
    fn uninstall_refuses_when_only_user_owned_config_exists() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".emacs.d")).unwrap();
        fs::write(home.join(".emacs.d/init.el"), "(setq user-config t)").unwrap();
        let source = make_source(temp.path());
        let manager = make_manager(&home, source, home.join(".emacs.d"));

        let err = manager.uninstall().unwrap_err();

        assert!(
            err.to_string()
                .contains("Refusing to remove non-Alan-owned")
        );
        assert!(home.join(".emacs.d/init.el").is_file());
    }

    #[test]
    fn doctor_accepts_connected_daemon_observation() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let source = make_source(temp.path());
        let default_dir = home.join(".emacs.d");
        let mut manager = make_manager(&home, source, default_dir);
        manager.probe.daemon = DaemonObservationFixture::Connected {
            raw: "(:daemon t :alan-emacs-loaded t)".to_string(),
            alan_loaded: true,
        };

        manager.install().unwrap();
        manager.doctor().unwrap();
    }
}
