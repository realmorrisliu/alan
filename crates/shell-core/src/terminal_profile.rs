mod launch;

pub use launch::{
    TerminalLaunchEnvironment, TerminalLaunchIntent, TerminalLaunchStrategy,
    TerminalProfileResolutionState, shell_quoted,
};

use serde::de::{self, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeSet;
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
        if let Some(existing_profile) = existing_managed_profile
            && !allows_managed_profile_repair_upsert(existing_profile, &draft)
        {
            return TerminalProfileDocumentEditorResult {
                document: None,
                errors: vec![TerminalProfileValidationError::ManagedProfileReadOnly {
                    id: draft_id.to_string(),
                }],
            };
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

/// Returns whether a global default profile should be captured on new panes.
pub fn should_capture_global_default_terminal_profile(profile: &TerminalProfileDefinition) -> bool {
    let _ = profile;
    false
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

fn normalized_non_empty(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
