use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Stable launch kind for Terminal Profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalProfileLaunchKind {
    /// Use the user's login shell.
    LoginShell,
    /// Launch a sudo login shell for a Unix user.
    SudoUser,
    /// Launch an interactive root shell.
    SudoRoot,
    /// Launch a helper-managed local user shell.
    ManagedUser,
    /// Run a custom command through zsh.
    CustomCommand,
}

/// Platform-neutral Terminal Profile launch definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalProfileLaunch {
    /// Use the user's login shell.
    LoginShell,
    /// Launch a sudo login shell for a Unix user.
    SudoUser {
        /// Target Unix user.
        unix_user: String,
    },
    /// Launch an interactive root shell.
    SudoRoot,
    /// Launch a helper-managed local user shell.
    ManagedUser {
        /// Target Unix user.
        unix_user: String,
    },
    /// Run a custom command through zsh.
    CustomCommand {
        /// Command payload.
        command: String,
    },
}

impl TerminalProfileLaunch {
    /// Returns the stable launch kind.
    pub fn kind(&self) -> TerminalProfileLaunchKind {
        match self {
            Self::LoginShell => TerminalProfileLaunchKind::LoginShell,
            Self::SudoUser { .. } => TerminalProfileLaunchKind::SudoUser,
            Self::SudoRoot => TerminalProfileLaunchKind::SudoRoot,
            Self::ManagedUser { .. } => TerminalProfileLaunchKind::ManagedUser,
            Self::CustomCommand { .. } => TerminalProfileLaunchKind::CustomCommand,
        }
    }

    /// Returns the Unix user for sudo-user launches.
    pub fn unix_user(&self) -> Option<&str> {
        match self {
            Self::SudoUser { unix_user } | Self::ManagedUser { unix_user } => Some(unix_user),
            _ => None,
        }
    }

    /// Returns the command for custom-command launches.
    pub fn custom_command(&self) -> Option<&str> {
        match self {
            Self::CustomCommand { command } => Some(command),
            _ => None,
        }
    }
}

impl Serialize for TerminalProfileLaunch {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("TerminalProfileLaunch", 3)?;
        state.serialize_field("kind", &self.kind())?;
        match self {
            Self::LoginShell | Self::SudoRoot => {}
            Self::SudoUser { unix_user } | Self::ManagedUser { unix_user } => {
                state.serialize_field("unix_user", unix_user)?
            }
            Self::CustomCommand { command } => state.serialize_field("command", command)?,
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for TerminalProfileLaunch {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            Kind,
            UnixUser,
            Command,
        }

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl Visitor<'_> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str("terminal profile launch field")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "kind" => Ok(Field::Kind),
                            "unix_user" => Ok(Field::UnixUser),
                            "command" => Ok(Field::Command),
                            _ => Err(de::Error::unknown_field(
                                value,
                                &["kind", "unix_user", "command"],
                            )),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct LaunchVisitor;

        impl<'de> Visitor<'de> for LaunchVisitor {
            type Value = TerminalProfileLaunch;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("terminal profile launch")
            }

            fn visit_map<M>(self, mut map: M) -> Result<TerminalProfileLaunch, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut kind = None;
                let mut unix_user = None;
                let mut command = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Kind => kind = Some(map.next_value()?),
                        Field::UnixUser => unix_user = Some(map.next_value()?),
                        Field::Command => command = Some(map.next_value()?),
                    }
                }

                match kind.ok_or_else(|| de::Error::missing_field("kind"))? {
                    TerminalProfileLaunchKind::LoginShell => Ok(TerminalProfileLaunch::LoginShell),
                    TerminalProfileLaunchKind::SudoUser => Ok(TerminalProfileLaunch::SudoUser {
                        unix_user: unix_user.unwrap_or_default(),
                    }),
                    TerminalProfileLaunchKind::SudoRoot => Ok(TerminalProfileLaunch::SudoRoot),
                    TerminalProfileLaunchKind::ManagedUser => {
                        Ok(TerminalProfileLaunch::ManagedUser {
                            unix_user: unix_user.unwrap_or_default(),
                        })
                    }
                    TerminalProfileLaunchKind::CustomCommand => {
                        Ok(TerminalProfileLaunch::CustomCommand {
                            command: command.unwrap_or_default(),
                        })
                    }
                }
            }
        }

        deserializer.deserialize_struct(
            "TerminalProfileLaunch",
            &["kind", "unix_user", "command"],
            LaunchVisitor,
        )
    }
}

/// Terminal Profile presentation metadata retained by shell core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfilePresentation {
    /// Symbol name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_name: Option<String>,
    /// Color name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_name: Option<String>,
}

/// Terminal Profile definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileDefinition {
    /// Stable profile id.
    pub id: String,
    /// User-visible title.
    pub title: String,
    /// Launch definition.
    pub launch: TerminalProfileLaunch,
    /// Optional default working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_working_directory: Option<String>,
    /// Optional presentation metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<TerminalProfilePresentation>,
    /// Optional managed account id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_terminal_account_id: Option<String>,
}

impl TerminalProfileDefinition {
    /// Login-shell fallback profile.
    pub fn login_shell_fallback() -> Self {
        Self {
            id: "login_shell".to_string(),
            title: "Login shell".to_string(),
            launch: TerminalProfileLaunch::LoginShell,
            default_working_directory: None,
            presentation: Some(TerminalProfilePresentation {
                symbol_name: Some("terminal".to_string()),
                color_name: None,
            }),
            managed_terminal_account_id: None,
        }
    }

    /// Redacted display detail for settings and diagnostics.
    pub fn redacted_display_detail(&self) -> String {
        match &self.launch {
            TerminalProfileLaunch::LoginShell => "Login shell".to_string(),
            TerminalProfileLaunch::SudoUser { unix_user } => {
                let trimmed = unix_user.trim();
                if trimmed.is_empty() {
                    "Sudo user".to_string()
                } else {
                    format!("Sudo user {unix_user}")
                }
            }
            TerminalProfileLaunch::SudoRoot => "Root shell".to_string(),
            TerminalProfileLaunch::ManagedUser { unix_user } => {
                let trimmed = unix_user.trim();
                if trimmed.is_empty() {
                    "Managed user".to_string()
                } else {
                    format!("Managed user {unix_user}")
                }
            }
            TerminalProfileLaunch::CustomCommand { .. } => "Custom command".to_string(),
        }
    }
}

/// Terminal Profile document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileDocument {
    /// Default profile id.
    pub default_profile_id: String,
    /// Profile definitions.
    pub profiles: Vec<TerminalProfileDefinition>,
}

impl TerminalProfileDocument {
    /// Fallback document.
    pub fn fallback() -> Self {
        let fallback = TerminalProfileDefinition::login_shell_fallback();
        Self {
            default_profile_id: fallback.id.clone(),
            profiles: vec![fallback],
        }
    }

    /// Finds a profile by id.
    pub fn profile(&self, id: Option<&str>) -> Option<&TerminalProfileDefinition> {
        let id = id?;
        self.profiles.iter().find(|profile| profile.id == id)
    }

    /// Returns the resolved default profile.
    pub fn default_profile(&self) -> Option<&TerminalProfileDefinition> {
        self.profile(Some(&self.default_profile_id))
            .or_else(|| self.profiles.first())
    }
}

/// Stable Terminal Profile validation error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalProfileValidationError {
    /// Profile id is empty.
    MissingId,
    /// Profile id is duplicated.
    DuplicateId {
        /// Duplicated id.
        id: String,
    },
    /// Profile title is empty.
    MissingTitle {
        /// Profile id.
        id: String,
    },
    /// sudo-user launch is missing the Unix user.
    MissingUnixUser {
        /// Profile id.
        id: String,
    },
    /// custom-command launch is missing the command.
    MissingCustomCommand {
        /// Profile id.
        id: String,
    },
    /// managed-user launch is missing a Managed User account id.
    MissingManagedAccount {
        /// Profile id.
        id: String,
    },
    /// managed-user launch target and Managed User account id disagree.
    ManagedAccountMismatch {
        /// Profile id.
        profile_id: String,
        /// Managed account id.
        account_id: String,
        /// Launch Unix user.
        unix_user: String,
    },
    /// Existing managed Terminal Profile is read-only.
    ManagedProfileReadOnly {
        /// Profile id.
        id: String,
    },
    /// Document default profile is missing.
    MissingDefaultProfile {
        /// Missing default profile id.
        id: String,
    },
    /// Required executable is unavailable.
    UnavailableExecutable {
        /// Profile id.
        profile_id: String,
        /// Executable path.
        path: String,
    },
}

impl TerminalProfileValidationError {
    /// User-facing message matching the Swift domain.
    pub fn user_message(&self) -> String {
        match self {
            Self::MissingId => "A Terminal Profile id is required.".to_string(),
            Self::DuplicateId { id } => format!("Terminal Profile {id} is duplicated."),
            Self::MissingTitle { id } => format!("Terminal Profile {id} needs a title."),
            Self::MissingUnixUser { id } => format!("Terminal Profile {id} needs a Unix user."),
            Self::MissingCustomCommand { id } => {
                format!("Terminal Profile {id} needs a custom command.")
            }
            Self::MissingManagedAccount { id } => {
                format!("Terminal Profile {id} needs a Managed User.")
            }
            Self::ManagedAccountMismatch {
                profile_id,
                account_id,
                unix_user,
            } => {
                format!(
                    "Terminal Profile {profile_id} links Managed User {account_id} but launches {unix_user}."
                )
            }
            Self::ManagedProfileReadOnly { id } => {
                format!("Managed Terminal Profile {id} is read-only.")
            }
            Self::MissingDefaultProfile { id } => {
                format!("Default Terminal Profile {id} is missing.")
            }
            Self::UnavailableExecutable { profile_id, path } => {
                format!("Terminal Profile {profile_id} cannot find executable {path}.")
            }
        }
    }
}

/// Terminal Profile validation result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileValidationResult {
    /// Validation errors.
    pub errors: Vec<TerminalProfileValidationError>,
}

impl TerminalProfileValidationResult {
    /// Returns whether the document is valid.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Optional executable availability supplied by a platform adapter.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalExecutableAvailability {
    /// Executable paths known to exist.
    #[serde(default)]
    pub executable_paths: BTreeSet<String>,
    /// Whether to enforce availability checks.
    #[serde(default)]
    pub enforce: bool,
}

impl TerminalExecutableAvailability {
    /// Creates an enforcing availability set.
    pub fn enforcing(paths: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            executable_paths: paths.into_iter().map(Into::into).collect(),
            enforce: true,
        }
    }

    fn is_executable(&self, path: &str) -> bool {
        !self.enforce || self.executable_paths.contains(path)
    }
}

/// Terminal Profile validator.
pub struct TerminalProfileValidator;

impl TerminalProfileValidator {
    /// Validates a document without platform filesystem access.
    pub fn validate(document: &TerminalProfileDocument) -> TerminalProfileValidationResult {
        Self::validate_with_availability(document, &TerminalExecutableAvailability::default())
    }

    /// Validates a document with platform-supplied executable availability.
    pub fn validate_with_availability(
        document: &TerminalProfileDocument,
        availability: &TerminalExecutableAvailability,
    ) -> TerminalProfileValidationResult {
        let mut errors = Vec::new();
        let mut ids = BTreeSet::new();

        for profile in &document.profiles {
            let trimmed_id = profile.id.trim();
            if trimmed_id.is_empty() {
                errors.push(TerminalProfileValidationError::MissingId);
            } else if !ids.insert(trimmed_id.to_string()) {
                errors.push(TerminalProfileValidationError::DuplicateId {
                    id: trimmed_id.to_string(),
                });
            }

            if profile.title.trim().is_empty() {
                errors.push(TerminalProfileValidationError::MissingTitle {
                    id: profile.id.clone(),
                });
            }

            match &profile.launch {
                TerminalProfileLaunch::LoginShell | TerminalProfileLaunch::SudoRoot => {}
                TerminalProfileLaunch::SudoUser { unix_user } => {
                    if unix_user.trim().is_empty() {
                        errors.push(TerminalProfileValidationError::MissingUnixUser {
                            id: profile.id.clone(),
                        });
                    }
                }
                TerminalProfileLaunch::ManagedUser { unix_user } => {
                    let trimmed_unix_user = unix_user.trim();
                    if trimmed_unix_user.is_empty() {
                        errors.push(TerminalProfileValidationError::MissingUnixUser {
                            id: profile.id.clone(),
                        });
                    }
                    match profile
                        .managed_terminal_account_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|account_id| !account_id.is_empty())
                    {
                        Some(account_id)
                            if !trimmed_unix_user.is_empty() && account_id != trimmed_unix_user =>
                        {
                            errors.push(TerminalProfileValidationError::ManagedAccountMismatch {
                                profile_id: profile.id.clone(),
                                account_id: account_id.to_string(),
                                unix_user: trimmed_unix_user.to_string(),
                            });
                        }
                        Some(_) => {}
                        None => {
                            errors.push(TerminalProfileValidationError::MissingManagedAccount {
                                id: profile.id.clone(),
                            });
                        }
                    }
                }
                TerminalProfileLaunch::CustomCommand { command } => {
                    if command.trim().is_empty() {
                        errors.push(TerminalProfileValidationError::MissingCustomCommand {
                            id: profile.id.clone(),
                        });
                    }
                }
            }

            if let Some(path) = Self::required_executable_path(&profile.launch)
                && !availability.is_executable(path)
            {
                errors.push(TerminalProfileValidationError::UnavailableExecutable {
                    profile_id: profile.id.clone(),
                    path: path.to_string(),
                });
            }
        }

        if !document.default_profile_id.is_empty() && !ids.contains(&document.default_profile_id) {
            errors.push(TerminalProfileValidationError::MissingDefaultProfile {
                id: document.default_profile_id.clone(),
            });
        }

        TerminalProfileValidationResult { errors }
    }

    /// Returns the executable path required by a launch definition.
    pub fn required_executable_path(launch: &TerminalProfileLaunch) -> Option<&'static str> {
        match launch {
            TerminalProfileLaunch::LoginShell => None,
            TerminalProfileLaunch::SudoUser { .. } | TerminalProfileLaunch::SudoRoot => {
                Some("/usr/bin/sudo")
            }
            TerminalProfileLaunch::ManagedUser { .. } => None,
            TerminalProfileLaunch::CustomCommand { .. } => Some("/bin/zsh"),
        }
    }
}

/// Editor draft for Terminal Profile definitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileEditorDraft {
    /// Profile id.
    pub id: String,
    /// Profile title.
    pub title: String,
    /// Launch kind.
    pub launch_kind: TerminalProfileLaunchKind,
    /// Unix user for sudo-user launches.
    pub unix_user: String,
    /// Command for custom-command launches.
    pub custom_command: String,
    /// Optional default working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_working_directory: Option<String>,
    /// Optional presentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<TerminalProfilePresentation>,
    /// Optional managed account id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_terminal_account_id: Option<String>,
}

impl TerminalProfileEditorDraft {
    /// Builds a draft from a definition.
    pub fn from_definition(profile: &TerminalProfileDefinition) -> Self {
        Self {
            id: profile.id.clone(),
            title: profile.title.clone(),
            launch_kind: profile.launch.kind(),
            unix_user: profile.launch.unix_user().unwrap_or_default().to_string(),
            custom_command: profile
                .launch
                .custom_command()
                .unwrap_or_default()
                .to_string(),
            default_working_directory: profile.default_working_directory.clone(),
            presentation: profile.presentation.clone(),
            managed_terminal_account_id: profile.managed_terminal_account_id.clone(),
        }
    }
}

/// Editor result for a single definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileEditorResult {
    /// Valid definition, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<TerminalProfileDefinition>,
    /// Validation errors.
    pub errors: Vec<TerminalProfileValidationError>,
}

impl TerminalProfileEditorResult {
    /// Returns whether the draft is valid.
    pub fn is_valid(&self) -> bool {
        self.definition.is_some() && self.errors.is_empty()
    }
}

/// Editor result for a document update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalProfileDocumentEditorResult {
    /// Valid document, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<TerminalProfileDocument>,
    /// Validation errors.
    pub errors: Vec<TerminalProfileValidationError>,
}

impl TerminalProfileDocumentEditorResult {
    /// Returns whether the document update is valid.
    pub fn is_valid(&self) -> bool {
        self.document.is_some() && self.errors.is_empty()
    }
}

/// Terminal Profile editor semantics.
pub struct TerminalProfileEditor;

impl TerminalProfileEditor {
    /// Creates a definition from an editor draft.
    pub fn make_definition(draft: TerminalProfileEditorDraft) -> TerminalProfileEditorResult {
        let launch = match draft.launch_kind {
            TerminalProfileLaunchKind::LoginShell => TerminalProfileLaunch::LoginShell,
            TerminalProfileLaunchKind::SudoUser => TerminalProfileLaunch::SudoUser {
                unix_user: draft.unix_user,
            },
            TerminalProfileLaunchKind::SudoRoot => TerminalProfileLaunch::SudoRoot,
            TerminalProfileLaunchKind::ManagedUser => TerminalProfileLaunch::ManagedUser {
                unix_user: draft.unix_user,
            },
            TerminalProfileLaunchKind::CustomCommand => TerminalProfileLaunch::CustomCommand {
                command: draft.custom_command,
            },
        };
        let definition = TerminalProfileDefinition {
            id: draft.id.trim().to_string(),
            title: draft.title.trim().to_string(),
            launch,
            default_working_directory: normalized_optional(draft.default_working_directory),
            presentation: draft.presentation,
            managed_terminal_account_id: normalized_optional(draft.managed_terminal_account_id),
        };
        let document = TerminalProfileDocument {
            default_profile_id: definition.id.clone(),
            profiles: vec![definition.clone()],
        };
        let validation = TerminalProfileValidator::validate(&document);
        TerminalProfileEditorResult {
            definition: validation.is_valid().then_some(definition),
            errors: validation.errors,
        }
    }

    /// Upserts a definition draft into a document.
    pub fn upsert(
        draft: TerminalProfileEditorDraft,
        document: &TerminalProfileDocument,
    ) -> TerminalProfileDocumentEditorResult {
        let draft_id = draft.id.trim();
        let existing_managed_profile = document.profiles.iter().find(|profile| {
            profile.id == draft_id
                && normalized_non_empty(profile.managed_terminal_account_id.as_deref()).is_some()
        });
        if let Some(existing_profile) = existing_managed_profile {
            if !allows_managed_profile_repair_upsert(existing_profile, &draft) {
                return TerminalProfileDocumentEditorResult {
                    document: None,
                    errors: vec![TerminalProfileValidationError::ManagedProfileReadOnly {
                        id: draft_id.to_string(),
                    }],
                };
            }
        }

        let editor_result = Self::make_definition(draft);
        let Some(definition) = editor_result.definition else {
            return TerminalProfileDocumentEditorResult {
                document: None,
                errors: editor_result.errors,
            };
        };

        let mut profiles = document.profiles.clone();
        if let Some(index) = profiles
            .iter()
            .position(|profile| profile.id == definition.id)
        {
            profiles[index] = definition;
        } else {
            profiles.push(definition);
        }
        let next_document = TerminalProfileDocument {
            default_profile_id: if document.default_profile_id.is_empty() {
                profiles
                    .last()
                    .map(|profile| profile.id.clone())
                    .unwrap_or_default()
            } else {
                document.default_profile_id.clone()
            },
            profiles,
        };
        let validation = TerminalProfileValidator::validate(&next_document);
        TerminalProfileDocumentEditorResult {
            document: validation.is_valid().then_some(next_document),
            errors: validation.errors,
        }
    }
}

/// Terminal Profile resolution state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TerminalProfileResolutionState {
    /// No profile was requested or applied.
    Absent,
    /// Profile resolved.
    Resolved,
    /// Requested profile is missing.
    Missing {
        /// Requested profile id.
        requested_id: String,
    },
    /// Requested profile is unavailable.
    Unavailable {
        /// Requested profile id.
        requested_id: String,
        /// Stable reason.
        reason: String,
    },
}

impl TerminalProfileResolutionState {
    /// Environment string value.
    pub fn environment_value(&self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Resolved => "resolved",
            Self::Missing { .. } => "missing",
            Self::Unavailable { .. } => "unavailable",
        }
    }
}

/// Launch strategy for terminal launch intents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalLaunchStrategy {
    /// ALAN_SHELL_BOOT_COMMAND controls startup.
    ShellCommandEnv,
    /// ALAN_SHELL_LOGIN_SHELL controls startup.
    LoginShellOverride,
    /// SHELL environment controls startup.
    LoginShellEnv,
    /// Fallback shell controls startup.
    LoginShellFallback,
    /// Terminal Profile sudo user launch.
    TerminalProfileSudoUser,
    /// Terminal Profile sudo root launch.
    TerminalProfileSudoRoot,
    /// Terminal Profile helper-managed user launch.
    TerminalProfileManagedUser,
    /// Terminal Profile custom command launch.
    TerminalProfileCustomCommand,
}

/// Platform-supplied launch environment.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLaunchEnvironment {
    /// Environment values.
    #[serde(default)]
    pub values: BTreeMap<String, String>,
}

impl TerminalLaunchEnvironment {
    fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

/// Platform-neutral launch intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalLaunchIntent {
    /// Launch strategy.
    pub strategy: TerminalLaunchStrategy,
    /// Executable path, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    /// Process launch path.
    pub launch_path: String,
    /// Process arguments.
    pub arguments: Vec<String>,
    /// Boot command string.
    pub boot_command: String,
    /// Optional terminal surface command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_command: Option<String>,
    /// User-facing summary.
    pub summary: String,
    /// Redacted detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Resolved Terminal Profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_profile: Option<TerminalProfileDefinition>,
    /// Terminal Profile resolution state.
    pub terminal_profile_state: TerminalProfileResolutionState,
    /// Working directory provided by the profile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// Environment projection for platform adapters.
    pub profile_environment: BTreeMap<String, String>,
}

struct TerminalLaunchIntentSeed {
    strategy: TerminalLaunchStrategy,
    executable_path: Option<String>,
    launch_path: String,
    arguments: Vec<String>,
    boot_command: String,
    surface_command: Option<String>,
    summary: String,
    detail: Option<String>,
    terminal_profile: Option<TerminalProfileDefinition>,
    terminal_profile_state: TerminalProfileResolutionState,
    working_directory: Option<String>,
}

impl TerminalLaunchIntent {
    /// Resolves a terminal launch intent.
    pub fn resolve(
        terminal_profile_reference: Option<&str>,
        terminal_profiles: Option<&TerminalProfileDocument>,
        availability: &TerminalExecutableAvailability,
        environment: &TerminalLaunchEnvironment,
    ) -> Self {
        let override_command = normalized_non_empty(environment.value("ALAN_SHELL_BOOT_COMMAND"));
        let override_shell =
            normalized_executable_path(environment.value("ALAN_SHELL_LOGIN_SHELL"), availability);
        if override_command.is_some() || override_shell.is_some() {
            return Self::resolve_shell(availability, environment);
        }

        let fallback;
        let document = match terminal_profiles {
            Some(document) => document,
            None => {
                fallback = TerminalProfileDocument::fallback();
                &fallback
            }
        };
        let requested_id = terminal_profile_reference.map(str::trim);
        let profile = requested_id
            .and_then(|id| document.profile(Some(id)))
            .or_else(|| {
                if requested_id.is_some_and(|id| !id.is_empty()) {
                    None
                } else {
                    document.default_profile()
                }
            });

        let Some(profile) = profile else {
            let state = requested_id
                .map(|requested_id| TerminalProfileResolutionState::Missing {
                    requested_id: requested_id.to_string(),
                })
                .unwrap_or(TerminalProfileResolutionState::Absent);
            return Self::resolve_shell(availability, environment)
                .with_terminal_profile(None, state);
        };

        match &profile.launch {
            TerminalProfileLaunch::LoginShell => Self::resolve_shell(availability, environment)
                .with_terminal_profile(
                    Some(profile.clone()),
                    TerminalProfileResolutionState::Resolved,
                ),
            TerminalProfileLaunch::SudoUser { unix_user } => Self::profile_command(
                profile,
                TerminalLaunchStrategy::TerminalProfileSudoUser,
                "/usr/bin/sudo",
                vec!["-iu".to_string(), unix_user.clone()],
                availability,
                environment,
            ),
            TerminalProfileLaunch::SudoRoot => Self::profile_command(
                profile,
                TerminalLaunchStrategy::TerminalProfileSudoRoot,
                "/usr/bin/sudo",
                vec!["-i".to_string()],
                availability,
                environment,
            ),
            TerminalProfileLaunch::ManagedUser { unix_user } => {
                let boot_command = shell_join(&["managed_user".to_string(), unix_user.clone()]);
                Self::from_seed(TerminalLaunchIntentSeed {
                    strategy: TerminalLaunchStrategy::TerminalProfileManagedUser,
                    executable_path: None,
                    launch_path: String::new(),
                    arguments: Vec::new(),
                    boot_command: boot_command.clone(),
                    surface_command: None,
                    summary: format!("Launching pane with Managed User {}", profile.title),
                    detail: Some(profile.redacted_display_detail()),
                    terminal_profile: Some(profile.clone()),
                    terminal_profile_state: TerminalProfileResolutionState::Resolved,
                    working_directory: normalized_non_empty(
                        profile.default_working_directory.as_deref(),
                    ),
                })
            }
            TerminalProfileLaunch::CustomCommand { command } => {
                let executable_path = "/bin/zsh";
                if !availability.is_executable(executable_path) {
                    return Self::resolve_shell(availability, environment).with_terminal_profile(
                        Some(profile.clone()),
                        TerminalProfileResolutionState::Unavailable {
                            requested_id: profile.id.clone(),
                            reason: "missing_executable".to_string(),
                        },
                    );
                }
                Self::from_seed(TerminalLaunchIntentSeed {
                    strategy: TerminalLaunchStrategy::TerminalProfileCustomCommand,
                    executable_path: Some(executable_path.to_string()),
                    launch_path: executable_path.to_string(),
                    arguments: vec!["-lc".to_string(), command.clone()],
                    boot_command: command.clone(),
                    surface_command: Some(command.clone()),
                    summary: format!("Launching pane with Terminal Profile {}", profile.title),
                    detail: Some(profile.redacted_display_detail()),
                    terminal_profile: Some(profile.clone()),
                    terminal_profile_state: TerminalProfileResolutionState::Resolved,
                    working_directory: normalized_non_empty(
                        profile.default_working_directory.as_deref(),
                    ),
                })
            }
        }
    }

    fn profile_command(
        profile: &TerminalProfileDefinition,
        strategy: TerminalLaunchStrategy,
        executable_path: &str,
        arguments: Vec<String>,
        availability: &TerminalExecutableAvailability,
        environment: &TerminalLaunchEnvironment,
    ) -> Self {
        if !availability.is_executable(executable_path) {
            return Self::resolve_shell(availability, environment).with_terminal_profile(
                Some(profile.clone()),
                TerminalProfileResolutionState::Unavailable {
                    requested_id: profile.id.clone(),
                    reason: "missing_executable".to_string(),
                },
            );
        }
        let boot_command = shell_join(
            std::iter::once(executable_path.to_string())
                .chain(arguments.iter().cloned())
                .collect::<Vec<_>>()
                .as_slice(),
        );
        Self::from_seed(TerminalLaunchIntentSeed {
            strategy,
            executable_path: Some(executable_path.to_string()),
            launch_path: executable_path.to_string(),
            arguments,
            boot_command: boot_command.clone(),
            surface_command: Some(boot_command),
            summary: format!("Launching pane with Terminal Profile {}", profile.title),
            detail: Some(profile.redacted_display_detail()),
            terminal_profile: Some(profile.clone()),
            terminal_profile_state: TerminalProfileResolutionState::Resolved,
            working_directory: normalized_non_empty(profile.default_working_directory.as_deref()),
        })
    }

    fn resolve_shell(
        availability: &TerminalExecutableAvailability,
        environment: &TerminalLaunchEnvironment,
    ) -> Self {
        if let Some(command) = normalized_non_empty(environment.value("ALAN_SHELL_BOOT_COMMAND")) {
            return Self::from_seed(TerminalLaunchIntentSeed {
                strategy: TerminalLaunchStrategy::ShellCommandEnv,
                executable_path: None,
                launch_path: "/bin/zsh".to_string(),
                arguments: vec!["-lc".to_string(), command.clone()],
                boot_command: command.clone(),
                surface_command: Some(command.clone()),
                summary: "Launching pane from ALAN_SHELL_BOOT_COMMAND".to_string(),
                detail: Some(command),
                terminal_profile: None,
                terminal_profile_state: TerminalProfileResolutionState::Absent,
                working_directory: None,
            });
        }

        if let Some(shell) =
            normalized_executable_path(environment.value("ALAN_SHELL_LOGIN_SHELL"), availability)
        {
            return Self::direct_shell(
                TerminalLaunchStrategy::LoginShellOverride,
                shell,
                "Launching pane from ALAN_SHELL_LOGIN_SHELL",
                false,
            );
        }

        if let Some(shell) = normalized_executable_path(environment.value("SHELL"), availability) {
            return Self::direct_shell(
                TerminalLaunchStrategy::LoginShellEnv,
                shell,
                "Launching pane from SHELL",
                true,
            );
        }

        let fallback_shell = ["/bin/zsh", "/bin/bash", "/bin/sh"]
            .into_iter()
            .find(|path| availability.is_executable(path))
            .unwrap_or("/bin/zsh")
            .to_string();
        Self::direct_shell(
            TerminalLaunchStrategy::LoginShellFallback,
            fallback_shell,
            "Launching pane with the default login shell",
            true,
        )
    }

    fn direct_shell(
        strategy: TerminalLaunchStrategy,
        executable_path: String,
        summary: &str,
        inherit_surface_command: bool,
    ) -> Self {
        let arguments = vec!["-l".to_string()];
        let boot_command = shell_join(&[executable_path.clone(), "-l".to_string()]);
        Self::from_seed(TerminalLaunchIntentSeed {
            strategy,
            executable_path: Some(executable_path.clone()),
            launch_path: executable_path.clone(),
            arguments,
            boot_command: boot_command.clone(),
            surface_command: (!inherit_surface_command).then_some(boot_command),
            summary: summary.to_string(),
            detail: Some(executable_path),
            terminal_profile: None,
            terminal_profile_state: TerminalProfileResolutionState::Absent,
            working_directory: None,
        })
    }

    fn from_seed(seed: TerminalLaunchIntentSeed) -> Self {
        let mut profile_environment = BTreeMap::new();
        profile_environment.insert(
            "ALAN_TERMINAL_PROFILE_STATE".to_string(),
            seed.terminal_profile_state.environment_value().to_string(),
        );
        if let TerminalProfileResolutionState::Missing { requested_id }
        | TerminalProfileResolutionState::Unavailable { requested_id, .. } =
            &seed.terminal_profile_state
        {
            profile_environment.insert(
                "ALAN_TERMINAL_PROFILE_REQUESTED_ID".to_string(),
                requested_id.clone(),
            );
        }
        if let Some(profile) = &seed.terminal_profile {
            profile_environment.insert("ALAN_TERMINAL_PROFILE_ID".to_string(), profile.id.clone());
            profile_environment.insert(
                "ALAN_TERMINAL_PROFILE_KIND".to_string(),
                profile.launch.kind().environment_value().to_string(),
            );
            if let TerminalProfileLaunch::ManagedUser { unix_user } = &profile.launch {
                profile_environment
                    .insert("ALAN_MANAGED_USER_ACCOUNT".to_string(), unix_user.clone());
            }
        }

        Self {
            strategy: seed.strategy,
            executable_path: seed.executable_path,
            launch_path: seed.launch_path,
            arguments: seed.arguments,
            boot_command: seed.boot_command,
            surface_command: seed.surface_command,
            summary: seed.summary,
            detail: seed.detail,
            terminal_profile: seed.terminal_profile,
            terminal_profile_state: seed.terminal_profile_state,
            working_directory: seed.working_directory,
            profile_environment,
        }
    }

    fn with_terminal_profile(
        mut self,
        profile: Option<TerminalProfileDefinition>,
        state: TerminalProfileResolutionState,
    ) -> Self {
        let working_directory = profile
            .as_ref()
            .and_then(|profile| normalized_non_empty(profile.default_working_directory.as_deref()));
        self.terminal_profile = profile;
        self.terminal_profile_state = state;
        self.working_directory = working_directory;
        self.profile_environment = Self::from_seed(TerminalLaunchIntentSeed {
            strategy: self.strategy,
            executable_path: self.executable_path.clone(),
            launch_path: self.launch_path.clone(),
            arguments: self.arguments.clone(),
            boot_command: self.boot_command.clone(),
            surface_command: self.surface_command.clone(),
            summary: self.summary.clone(),
            detail: self.detail.clone(),
            terminal_profile: self.terminal_profile.clone(),
            terminal_profile_state: self.terminal_profile_state.clone(),
            working_directory: self.working_directory.clone(),
        })
        .profile_environment;
        self
    }
}

impl TerminalProfileLaunchKind {
    fn environment_value(self) -> &'static str {
        match self {
            TerminalProfileLaunchKind::LoginShell => "login_shell",
            TerminalProfileLaunchKind::SudoUser => "sudo_user",
            TerminalProfileLaunchKind::SudoRoot => "sudo_root",
            TerminalProfileLaunchKind::ManagedUser => "managed_user",
            TerminalProfileLaunchKind::CustomCommand => "custom_command",
        }
    }
}

/// Returns whether a global default profile should be captured on new panes.
pub fn should_capture_global_default_terminal_profile(profile: &TerminalProfileDefinition) -> bool {
    let _ = profile;
    false
}

/// Portable request to prepare a managed local terminal account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountRequest {
    /// Managed account short name.
    pub account_name: String,
    /// GUI user allowed to enter this account through sudo.
    pub gui_user_name: String,
    /// Optional display name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// Login shell path.
    pub shell: String,
    /// Managed account home directory.
    pub home_directory: String,
    /// Whether the account should be hidden from login window lists.
    pub hide_from_login_window: bool,
    /// Whether to bind the current Space after provisioning succeeds.
    pub bind_current_space_after_success: bool,
}

impl ManagedTerminalAccountRequest {
    /// Returns the canonical home directory for a managed account name.
    pub fn canonical_home_directory(account_name: &str) -> String {
        format!("/Users/{account_name}")
    }

    /// Returns the Terminal Profile id associated with this managed account.
    pub fn terminal_profile_id(&self) -> &str {
        &self.account_name
    }
}

/// Stable validation error for managed terminal account requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManagedTerminalAccountValidationError {
    /// Managed account name is invalid.
    InvalidAccountName {
        /// Invalid value.
        value: String,
    },
    /// GUI user name is invalid.
    InvalidGuiUserName {
        /// Invalid value.
        value: String,
    },
    /// Managed account name is reserved.
    ReservedAccountName {
        /// Reserved value.
        value: String,
    },
    /// Shell path is invalid.
    InvalidShell {
        /// Invalid value.
        value: String,
    },
}

/// Validator for managed terminal account request identifiers.
pub struct ManagedTerminalAccountIdentifierValidator;

impl ManagedTerminalAccountIdentifierValidator {
    /// Validates portable request fields without querying the host OS.
    pub fn validate(
        request: &ManagedTerminalAccountRequest,
    ) -> Vec<ManagedTerminalAccountValidationError> {
        let mut errors = Vec::new();
        if !matches_managed_account_identifier(&request.account_name) {
            errors.push(ManagedTerminalAccountValidationError::InvalidAccountName {
                value: request.account_name.clone(),
            });
        }
        if matches!(
            request.account_name.to_ascii_lowercase().as_str(),
            "root" | "daemon" | "nobody"
        ) {
            errors.push(ManagedTerminalAccountValidationError::ReservedAccountName {
                value: request.account_name.clone(),
            });
        }
        if !matches_managed_account_identifier(&request.gui_user_name) {
            errors.push(ManagedTerminalAccountValidationError::InvalidGuiUserName {
                value: request.gui_user_name.clone(),
            });
        }
        if request.shell.trim().is_empty() || request.shell.contains('\n') {
            errors.push(ManagedTerminalAccountValidationError::InvalidShell {
                value: request.shell.clone(),
            });
        }
        errors
    }
}

/// Discovered managed account state supplied by a platform adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountRecord {
    /// Account does not exist.
    Missing,
    /// Account exists as a standard user.
    Standard {
        /// Current home directory.
        home_directory: String,
        /// Current shell.
        shell: String,
        /// Whether the account is hidden.
        hidden: bool,
    },
    /// Account exists but is an administrator.
    Admin {
        /// Current home directory.
        home_directory: String,
        /// Current shell.
        shell: String,
        /// Whether the account is hidden.
        hidden: bool,
    },
    /// Account exists but has an invalid or unreadable state.
    Invalid {
        /// Redacted reason.
        reason: String,
    },
}

impl ManagedTerminalAccountRecord {
    fn requires_alan_managed_ownership(&self) -> bool {
        matches!(self, Self::Standard { .. } | Self::Admin { .. })
    }
}

/// Discovered sudoers drop-in state supplied by a platform adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountSudoersState {
    /// Drop-in is absent.
    Missing,
    /// Alan-owned drop-in is valid.
    AlanOwnedValid {
        /// Drop-in path.
        path: String,
    },
    /// Alan-owned drop-in is present but invalid.
    AlanOwnedInvalid {
        /// Drop-in path.
        path: String,
        /// Redacted validation message.
        message: String,
    },
    /// Drop-in path is occupied by unmanaged content.
    Unmanaged {
        /// Drop-in path.
        path: String,
    },
    /// Drop-in path exists but could not be read.
    ExistingUnreadable {
        /// Drop-in path.
        path: String,
    },
}

/// Evidence that an existing account is owned by Alan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountOwnershipEvidence {
    /// Helper-owned marker file.
    HelperMarker {
        /// Marker path.
        path: String,
    },
    /// Legacy Alan sudoers state.
    LegacyAlanSudoers {
        /// Sudoers path.
        path: String,
    },
}

/// Ownership state for an existing managed terminal account.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountOwnershipState {
    /// No ownership evidence is present.
    #[default]
    Missing,
    /// Alan owns this account.
    AlanManaged {
        /// Ownership evidence.
        evidence: ManagedTerminalAccountOwnershipEvidence,
    },
    /// The account exists but is outside Alan management.
    NotAlanManaged {
        /// Redacted reason.
        reason: String,
    },
}

impl ManagedTerminalAccountOwnershipState {
    fn is_alan_managed(&self) -> bool {
        matches!(self, Self::AlanManaged { .. })
    }
}

/// Discovered managed Terminal Profile state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ManagedTerminalAccountProfileState {
    /// Profile is absent.
    Missing,
    /// Matching managed profile exists.
    ExistingManaged {
        /// Profile id.
        profile_id: String,
    },
    /// Matching managed profile exists but needs refresh.
    ExistingManagedOutdated {
        /// Profile id.
        profile_id: String,
    },
    /// Profile id exists but is not managed by this request.
    ExistingUnmanaged {
        /// Profile id.
        profile_id: String,
    },
}

/// Readiness verification step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedTerminalAccountVerificationStep {
    /// Account lookup.
    AccountLookup,
    /// Non-admin account requirement.
    NonAdminAccount,
    /// Home directory check.
    HomeDirectory,
    /// Shell check.
    Shell,
    /// Alan ownership check.
    Ownership,
    /// Sudoers syntax check.
    SudoersValidation,
    /// Non-interactive sudo check.
    NonInteractiveSudo,
}

/// Readiness verification status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ManagedTerminalAccountVerificationStatus {
    /// Verification has not run.
    NotRun,
    /// Verification passed.
    Passed,
    /// Verification failed.
    Failed {
        /// Failed step.
        step: ManagedTerminalAccountVerificationStep,
        /// Redacted message.
        message: String,
    },
}

/// Portable discovered managed terminal account state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountState {
    /// Account record.
    pub account: ManagedTerminalAccountRecord,
    /// Sudoers record.
    pub sudoers: ManagedTerminalAccountSudoersState,
    /// Alan ownership evidence for existing accounts.
    #[serde(default)]
    pub ownership: ManagedTerminalAccountOwnershipState,
    /// Terminal Profile record.
    pub terminal_profile: ManagedTerminalAccountProfileState,
    /// Verification status.
    pub verification: ManagedTerminalAccountVerificationStatus,
    /// Whether the account home directory exists on disk.
    #[serde(default = "default_true")]
    pub home_directory_exists: bool,
}

/// Managed terminal account plan step kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedTerminalAccountPlanStepKind {
    /// Create a standard account.
    CreateStandardAccount,
    /// Repair account type.
    RepairAccountType,
    /// Repair home directory.
    RepairHomeDirectory,
    /// Repair shell.
    RepairShell,
    /// Hide account from login window lists.
    HideAccount,
    /// Write sudoers drop-in.
    WriteSudoersDropIn,
    /// Validate sudoers drop-in.
    ValidateSudoers,
    /// Verify terminal entry.
    VerifyTerminalEntry,
    /// Create or update Terminal Profile handoff.
    CreateOrUpdateTerminalProfile,
    /// Bind current Space to profile.
    BindCurrentSpace,
    /// Remove sudoers drop-in.
    RemoveSudoersDropIn,
    /// Remove managed Terminal Profile.
    RemoveManagedTerminalProfile,
    /// Delete the account.
    DeleteAccount,
    /// Delete the account home directory.
    DeleteHomeDirectory,
    /// Write the helper ownership marker.
    WriteOwnershipMarker,
    /// Verify the helper-managed account state.
    VerifyAccount,
    /// Clean up a verified legacy Alan sudoers file.
    CleanupLegacySudoers,
    /// Verify helper-managed PTY startup.
    VerifyManagedUserPty,
    /// Remove helper-managed account integration.
    RemoveManagedUserIntegration,
}

/// Managed terminal account plan step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountPlanStep {
    /// Step kind.
    pub kind: ManagedTerminalAccountPlanStepKind,
    /// User-facing summary.
    pub summary: String,
    /// Whether the step requires privilege.
    pub requires_privilege: bool,
}

/// Managed terminal account plan status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ManagedTerminalAccountPlanStatus {
    /// Plan is ready to apply.
    ReadyToApply,
    /// Nothing needs to change.
    AlreadyReady,
    /// Existing state needs repair.
    Repair,
    /// Request is invalid.
    Invalid {
        /// Validation errors.
        errors: Vec<ManagedTerminalAccountValidationError>,
    },
    /// Destructive rollback requires confirmation.
    RequiresDestructiveConfirmation,
    /// Sudoers path is occupied by unmanaged content.
    SudoersConflict {
        /// Conflicting path.
        path: String,
    },
    /// Terminal Profile id is occupied by unmanaged content.
    TerminalProfileConflict {
        /// Conflicting profile id.
        profile_id: String,
    },
    /// Privileged helper is unavailable.
    HelperUnavailable,
    /// Account exists but is not Alan-managed.
    AccountNotAlanManaged,
    /// A legacy Alan sudoers file needs cleanup.
    LegacySudoersPresent {
        /// Optional legacy sudoers path.
        path: Option<String>,
    },
    /// Helper-managed PTY smoke failed.
    PtySpawnFailed,
}

impl ManagedTerminalAccountPlanStatus {
    /// Stable label used by settings summaries and diagnostics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReadyToApply => "ready_to_apply",
            Self::AlreadyReady => "already_ready",
            Self::Repair => "repair",
            Self::Invalid { .. } => "invalid",
            Self::RequiresDestructiveConfirmation => "requires_destructive_confirmation",
            Self::SudoersConflict { .. } => "sudoers_conflict",
            Self::TerminalProfileConflict { .. } => "terminal_profile_conflict",
            Self::HelperUnavailable => "helper_unavailable",
            Self::AccountNotAlanManaged => "account_not_alan_managed",
            Self::LegacySudoersPresent { .. } => "legacy_sudoers_present",
            Self::PtySpawnFailed => "pty_spawn_failed",
        }
    }
}

/// Managed terminal account plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountPlan {
    /// Original request.
    pub request: ManagedTerminalAccountRequest,
    /// Plan status.
    pub status: ManagedTerminalAccountPlanStatus,
    /// Plan steps.
    pub steps: Vec<ManagedTerminalAccountPlanStep>,
}

/// Planner for portable managed terminal account semantics.
pub struct ManagedTerminalAccountPlanner;

impl ManagedTerminalAccountPlanner {
    /// Builds a provisioning or repair plan from platform-supplied state.
    pub fn plan(
        request: ManagedTerminalAccountRequest,
        state: &ManagedTerminalAccountState,
    ) -> ManagedTerminalAccountPlan {
        let validation_errors = ManagedTerminalAccountIdentifierValidator::validate(&request);
        if !validation_errors.is_empty() {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::Invalid {
                    errors: validation_errors,
                },
                steps: Vec::new(),
            };
        }
        if state.account.requires_alan_managed_ownership() && !state.ownership.is_alan_managed() {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::AccountNotAlanManaged,
                steps: Vec::new(),
            };
        }
        if let ManagedTerminalAccountSudoersState::Unmanaged { path } = &state.sudoers {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::SudoersConflict { path: path.clone() },
                steps: Vec::new(),
            };
        }
        if let ManagedTerminalAccountProfileState::ExistingUnmanaged { profile_id } =
            &state.terminal_profile
        {
            return ManagedTerminalAccountPlan {
                request,
                status: ManagedTerminalAccountPlanStatus::TerminalProfileConflict {
                    profile_id: profile_id.clone(),
                },
                steps: Vec::new(),
            };
        }

        let mut steps = Vec::new();
        let mut needs_create = false;
        let mut repair_needed = false;

        match &state.account {
            ManagedTerminalAccountRecord::Missing => {
                needs_create = true;
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::CreateStandardAccount,
                    "Create standard local terminal account",
                    true,
                ));
                if request.hide_from_login_window {
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::HideAccount,
                        "Hide terminal account from login window lists",
                        true,
                    ));
                }
            }
            ManagedTerminalAccountRecord::Admin { .. } => {
                repair_needed = true;
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::RepairAccountType,
                    "Repair account so it is not an administrator",
                    true,
                ));
            }
            ManagedTerminalAccountRecord::Invalid { reason } => {
                if reason.to_ascii_lowercase().contains("incomplete") {
                    needs_create = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::CreateStandardAccount,
                        format!("Complete local terminal account record: {reason}"),
                        true,
                    ));
                    if request.hide_from_login_window {
                        steps.push(managed_account_step(
                            ManagedTerminalAccountPlanStepKind::HideAccount,
                            "Hide terminal account from login window lists",
                            true,
                        ));
                    }
                } else {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::RepairAccountType,
                        format!("Repair account state: {reason}"),
                        true,
                    ));
                }
            }
            ManagedTerminalAccountRecord::Standard {
                home_directory,
                shell,
                hidden,
            } => {
                if home_directory != &request.home_directory || !state.home_directory_exists {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::RepairHomeDirectory,
                        "Repair terminal account home directory",
                        true,
                    ));
                }
                if shell != &request.shell {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::RepairShell,
                        "Repair terminal account shell",
                        true,
                    ));
                }
                if request.hide_from_login_window && !hidden {
                    repair_needed = true;
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::HideAccount,
                        "Hide terminal account from login window lists",
                        true,
                    ));
                }
            }
        }

        match &state.sudoers {
            ManagedTerminalAccountSudoersState::Missing
            | ManagedTerminalAccountSudoersState::AlanOwnedInvalid { .. } => {
                if !needs_create {
                    repair_needed = true;
                }
                append_sudoers_write_steps(&mut steps);
            }
            ManagedTerminalAccountSudoersState::AlanOwnedValid { .. }
            | ManagedTerminalAccountSudoersState::ExistingUnreadable { .. }
            | ManagedTerminalAccountSudoersState::Unmanaged { .. } => {}
        }

        if state.verification != ManagedTerminalAccountVerificationStatus::Passed {
            if !needs_create {
                repair_needed = true;
            }
            if should_repair_unreadable_sudoers(state) {
                append_sudoers_write_steps(&mut steps);
            }
            steps.push(managed_account_step(
                ManagedTerminalAccountPlanStepKind::VerifyTerminalEntry,
                "Verify passwordless terminal entry",
                false,
            ));
        }

        match &state.terminal_profile {
            ManagedTerminalAccountProfileState::Missing => {
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
                    "Create matching Terminal Profile",
                    false,
                ));
            }
            ManagedTerminalAccountProfileState::ExistingManaged { .. } => {
                if state.verification != ManagedTerminalAccountVerificationStatus::Passed {
                    steps.push(managed_account_step(
                        ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
                        "Refresh matching Terminal Profile",
                        false,
                    ));
                }
            }
            ManagedTerminalAccountProfileState::ExistingManagedOutdated { .. } => {
                repair_needed = true;
                steps.push(managed_account_step(
                    ManagedTerminalAccountPlanStepKind::CreateOrUpdateTerminalProfile,
                    "Refresh matching Terminal Profile",
                    false,
                ));
            }
            ManagedTerminalAccountProfileState::ExistingUnmanaged { .. } => {}
        }

        if request.bind_current_space_after_success {
            steps.push(managed_account_step(
                ManagedTerminalAccountPlanStepKind::BindCurrentSpace,
                "Bind current Space after confirmation",
                false,
            ));
        }

        let status = if steps.is_empty() {
            ManagedTerminalAccountPlanStatus::AlreadyReady
        } else if repair_needed {
            ManagedTerminalAccountPlanStatus::Repair
        } else {
            ManagedTerminalAccountPlanStatus::ReadyToApply
        };
        ManagedTerminalAccountPlan {
            request,
            status,
            steps,
        }
    }
}

/// Alan-owned sudoers drop-in projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountSudoersRule {
    /// Sudoers drop-in file name.
    pub file_name: String,
    /// Sudoers drop-in absolute path.
    pub file_path: String,
    /// Sudoers drop-in contents.
    pub contents: String,
}

impl ManagedTerminalAccountSudoersRule {
    /// Marker used to identify Alan-owned sudoers content.
    pub const MANAGED_MARKER: &'static str =
        "# Managed by Alan for terminal account entry. Do not edit by hand.";

    /// Builds the deterministic sudoers rule for a request.
    pub fn new(request: &ManagedTerminalAccountRequest) -> Self {
        let file_name = format!(
            "alan-terminal-{}-to-{}",
            request.gui_user_name, request.account_name
        );
        let file_path = format!("/etc/sudoers.d/{file_name}");
        let contents = format!(
            "{}\n{} ALL=({}) NOPASSWD: ALL",
            Self::MANAGED_MARKER,
            request.gui_user_name,
            request.account_name
        );
        Self {
            file_name,
            file_path,
            contents,
        }
    }
}

/// Result projected by a managed account dry-run executor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagedTerminalAccountApplyResult {
    /// Completed steps.
    pub completed_steps: Vec<ManagedTerminalAccountPlanStepKind>,
    /// Failed step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_step: Option<ManagedTerminalAccountPlanStepKind>,
    /// Whether the operation was cancelled.
    pub cancelled: bool,
    /// Redacted visible diagnostics.
    pub visible_diagnostics: Vec<String>,
}

impl ManagedTerminalAccountApplyResult {
    /// Returns the Swift-compatible dry-run cancellation result.
    pub fn cancelled(before: &[ManagedTerminalAccountPlanStepKind]) -> Self {
        Self {
            completed_steps: Vec::new(),
            failed_step: before.first().copied(),
            cancelled: true,
            visible_diagnostics: vec![
                "Provisioning cancelled before privileged changes.".to_string(),
            ],
        }
    }
}

/// Fake executor for deterministic managed-account dry-run projections.
pub struct ManagedTerminalAccountFakeExecutor;

impl ManagedTerminalAccountFakeExecutor {
    /// Applies a plan in memory, optionally cancelling before the first step.
    pub fn apply(
        plan: &ManagedTerminalAccountPlan,
        cancel_before_apply: bool,
        fail_at: Option<ManagedTerminalAccountPlanStepKind>,
    ) -> ManagedTerminalAccountApplyResult {
        let step_kinds = plan.steps.iter().map(|step| step.kind).collect::<Vec<_>>();
        if cancel_before_apply {
            return ManagedTerminalAccountApplyResult::cancelled(&step_kinds);
        }

        let mut completed_steps = Vec::new();
        for step in &plan.steps {
            if Some(step.kind) == fail_at {
                return ManagedTerminalAccountApplyResult {
                    completed_steps,
                    failed_step: Some(step.kind),
                    cancelled: false,
                    visible_diagnostics: vec![format!(
                        "Step failed: {}. Credentials redacted.",
                        step.summary
                    )],
                };
            }
            completed_steps.push(step.kind);
        }
        ManagedTerminalAccountApplyResult {
            completed_steps,
            failed_step: None,
            cancelled: false,
            visible_diagnostics: vec![
                "Provisioning plan applied. Credentials redacted.".to_string(),
            ],
        }
    }
}

/// Terminal Profile handoff helper for managed terminal accounts.
pub struct ManagedTerminalAccountProfileHandoff;

impl ManagedTerminalAccountProfileHandoff {
    /// Produces a managed Terminal Profile once readiness verification has passed.
    pub fn profile_definition(
        request: &ManagedTerminalAccountRequest,
        state: &ManagedTerminalAccountState,
    ) -> Option<TerminalProfileDefinition> {
        (state.verification == ManagedTerminalAccountVerificationStatus::Passed).then(|| {
            TerminalProfileDefinition {
                id: request.terminal_profile_id().to_string(),
                title: request
                    .full_name
                    .clone()
                    .unwrap_or_else(|| request.account_name.clone()),
                launch: TerminalProfileLaunch::ManagedUser {
                    unix_user: request.account_name.clone(),
                },
                default_working_directory: Some(request.home_directory.clone()),
                presentation: Some(TerminalProfilePresentation {
                    symbol_name: Some("person.crop.circle".to_string()),
                    color_name: None,
                }),
                managed_terminal_account_id: Some(request.account_name.clone()),
            }
        })
    }
}

fn normalized_optional(value: Option<String>) -> Option<String> {
    normalized_non_empty(value.as_deref())
}

fn allows_managed_profile_repair_upsert(
    existing: &TerminalProfileDefinition,
    draft: &TerminalProfileEditorDraft,
) -> bool {
    let Some(existing_account_id) =
        normalized_non_empty(existing.managed_terminal_account_id.as_deref())
    else {
        return false;
    };
    let Some(draft_account_id) = normalized_non_empty(draft.managed_terminal_account_id.as_deref())
    else {
        return false;
    };
    draft.launch_kind == TerminalProfileLaunchKind::ManagedUser
        && draft_account_id == existing_account_id
        && draft.unix_user.trim() == existing_account_id
}

fn matches_managed_account_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    let mut len = 1;
    for ch in chars {
        len += 1;
        if len > 32 || !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
            return false;
        }
    }
    true
}

fn managed_account_step(
    kind: ManagedTerminalAccountPlanStepKind,
    summary: impl Into<String>,
    requires_privilege: bool,
) -> ManagedTerminalAccountPlanStep {
    ManagedTerminalAccountPlanStep {
        kind,
        summary: summary.into(),
        requires_privilege,
    }
}

fn default_true() -> bool {
    true
}

fn append_sudoers_write_steps(steps: &mut Vec<ManagedTerminalAccountPlanStep>) {
    steps.push(managed_account_step(
        ManagedTerminalAccountPlanStepKind::WriteSudoersDropIn,
        "Write Alan-owned sudoers drop-in",
        true,
    ));
    steps.push(managed_account_step(
        ManagedTerminalAccountPlanStepKind::ValidateSudoers,
        "Validate sudoers syntax",
        true,
    ));
}

fn should_repair_unreadable_sudoers(state: &ManagedTerminalAccountState) -> bool {
    matches!(
        (&state.sudoers, &state.verification),
        (
            ManagedTerminalAccountSudoersState::ExistingUnreadable { .. },
            ManagedTerminalAccountVerificationStatus::Failed {
                step: ManagedTerminalAccountVerificationStep::NonInteractiveSudo,
                ..
            }
        )
    )
}

fn normalized_non_empty(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalized_executable_path(
    raw_path: Option<&str>,
    availability: &TerminalExecutableAvailability,
) -> Option<String> {
    let path = normalized_non_empty(raw_path)?;
    availability.is_executable(&path).then_some(path)
}

fn shell_join(values: &[String]) -> String {
    values
        .iter()
        .map(|value| shell_quoted(value))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Quotes a shell argument using the Swift boot-profile convention.
pub fn shell_quoted(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}
