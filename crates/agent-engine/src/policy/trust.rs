//! Explicit invocation-only trust posture for Alan Process execution.
//!
//! Trust posture controls whether an Alan invocation uses adaptive confined
//! smart review (`--smart`), relaxes per-action authorization within the OS
//! confinement boundary (`--trust`), or opts into unsandboxed native
//! Host-account execution (`--trust --unsafe-host-account`).
//! It is strictly an in-memory host/invocation parameter and MUST NEVER be
//! parsed from repository configuration, `AGENTS.md`/`CLAUDE.md`, saved rollout
//! evidence, or AgentFS control files.

use serde_json::json;
use thiserror::Error;

/// Explicit invocation-only trust posture governing tool execution and policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrustPosture {
    /// Default governed execution inside the OS boundary with policy, red-line,
    /// and Guardian / human escalation on uncontained or boundary-crossing effects.
    #[default]
    Governed,
    /// Explicit `--smart`: keep the OS sandbox and explicit Host Mount bounds active,
    /// retain per-action approval checks for high-risk or boundary-crossing actions,
    /// and allow adaptive smart review inside the confinement boundary.
    ConfinedSmart,
    /// Explicit `--trust`: keep the OS sandbox and explicit Host Mount bounds active,
    /// bypass per-action policy and Guardian prompts (including action red lines),
    /// and never silently widen OS sandbox permissions.
    ConfinedTrusted,
    /// Explicit `--trust --unsafe-host-account`: disable the OS sandbox and
    /// Guardian prompts to run directly with the invoking user's Host account.
    /// Does not claim confinement or tamperproof same-account audit guarantees.
    HostAccountTrusted,
}

/// Validation failure when resolving an invocation or child trust posture.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TrustPostureError {
    /// `--unsafe-host-account` was passed without `--trust`.
    #[error("`--unsafe-host-account` requires `--trust`")]
    UnsafeHostAccountWithoutTrust,
    /// `--smart` was passed together with `--trust`.
    #[error("`--smart` cannot be combined with `--trust`")]
    SmartConflictsWithTrust,
    /// `--smart` was passed together with `--unsafe-host-account`.
    #[error("`--smart` cannot be combined with `--unsafe-host-account`")]
    SmartConflictsWithUnsafeHostAccount,
    /// A child launch requested a trust posture broader than its parent's.
    #[error(
        "child trust posture `{requested}` is broader than parent posture `{parent}`; \
         children may only inherit or narrow parent trust"
    )]
    ChildBroaderThanParent {
        /// Requested child posture label.
        requested: &'static str,
        /// Parent posture label.
        parent: &'static str,
    },
    /// Unrecognized trust posture token passed across an invocation boundary.
    #[error(
        "invalid trust posture `{0}`; expected governed, confined_smart, confined_trusted, or host_account_trusted"
    )]
    InvalidPostureToken(String),
}

impl TrustPosture {
    /// Resolve posture exclusively from explicit CLI/host invocation flags.
    pub fn from_cli_flags(
        trust: bool,
        unsafe_host_account: bool,
    ) -> Result<Self, TrustPostureError> {
        Self::from_cli_posture(false, trust, unsafe_host_account)
    }

    /// Resolve posture exclusively from explicit `--smart`, `--trust`, and
    /// `--unsafe-host-account` CLI/host invocation flags.
    pub fn from_cli_posture(
        smart: bool,
        trust: bool,
        unsafe_host_account: bool,
    ) -> Result<Self, TrustPostureError> {
        match (smart, trust, unsafe_host_account) {
            (false, false, false) => Ok(Self::Governed),
            (true, false, false) => Ok(Self::ConfinedSmart),
            (false, true, false) => Ok(Self::ConfinedTrusted),
            (false, true, true) => Ok(Self::HostAccountTrusted),
            (false, false, true) => Err(TrustPostureError::UnsafeHostAccountWithoutTrust),
            (true, true, _) => Err(TrustPostureError::SmartConflictsWithTrust),
            (true, false, true) => Err(TrustPostureError::SmartConflictsWithUnsafeHostAccount),
        }
    }

    /// Parse an internal host-boot canonical token (never for repo config files).
    pub fn from_internal_token(token: &str) -> Result<Self, TrustPostureError> {
        match token {
            "governed" => Ok(Self::Governed),
            "confined_smart" => Ok(Self::ConfinedSmart),
            "confined_trusted" => Ok(Self::ConfinedTrusted),
            "host_account_trusted" => Ok(Self::HostAccountTrusted),
            other => Err(TrustPostureError::InvalidPostureToken(other.to_string())),
        }
    }

    /// Canonical machine-readable posture identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Governed => "governed",
            Self::ConfinedSmart => "confined_smart",
            Self::ConfinedTrusted => "confined_trusted",
            Self::HostAccountTrusted => "host_account_trusted",
        }
    }

    /// Privilege rank used to enforce equal-or-narrower child posture bounds.
    const fn rank(self) -> u8 {
        match self {
            Self::Governed => 0,
            Self::ConfinedSmart => 1,
            Self::ConfinedTrusted => 2,
            Self::HostAccountTrusted => 3,
        }
    }

    /// Returns `true` when `self` does not broaden authority relative to `parent`.
    pub const fn is_equal_or_narrower_than(self, parent: Self) -> bool {
        self.rank() <= parent.rank()
    }

    /// Resolve a child's effective posture against its parent's posture.
    ///
    /// When `requested` is `None`, the child inherits `parent`. When `Some`,
    /// `requested` must be equal to or narrower than `parent`.
    pub fn resolve_child(parent: Self, requested: Option<Self>) -> Result<Self, TrustPostureError> {
        match requested {
            None => Ok(parent),
            Some(child) if child.is_equal_or_narrower_than(parent) => Ok(child),
            Some(child) => Err(TrustPostureError::ChildBroaderThanParent {
                requested: child.as_str(),
                parent: parent.as_str(),
            }),
        }
    }

    /// Returns the narrower of `self` and `other`.
    pub const fn narrowest(self, other: Self) -> Self {
        if self.rank() <= other.rank() {
            self
        } else {
            other
        }
    }

    /// Whether OS sandbox confinement remains active (`Governed`, `ConfinedSmart`, and `ConfinedTrusted`).
    pub const fn uses_os_sandbox(self) -> bool {
        !matches!(self, Self::HostAccountTrusted)
    }

    /// Whether Host Mount access bounds apply to sandboxed operations.
    pub const fn enforces_host_mount_bounds(self) -> bool {
        !matches!(self, Self::HostAccountTrusted)
    }

    /// Whether per-action policy prompts, red lines, and Guardian reviews are bypassed.
    pub const fn bypasses_action_approvals(self) -> bool {
        matches!(self, Self::ConfinedTrusted | Self::HostAccountTrusted)
    }

    /// Whether `self` runs unsandboxed directly under the invoking user's Host account.
    pub const fn is_unsandboxed_host_account(self) -> bool {
        matches!(self, Self::HostAccountTrusted)
    }

    /// Whether `self` may legitimately claim an OS-enforced confinement boundary.
    pub const fn offers_tamperproof_confinement(self) -> bool {
        matches!(
            self,
            Self::Governed | Self::ConfinedSmart | Self::ConfinedTrusted
        )
    }

    /// Short TUI context-line badge label when non-default posture is active.
    pub const fn tui_badge_label(self) -> Option<&'static str> {
        match self {
            Self::Governed => None,
            Self::ConfinedSmart => Some("smart"),
            Self::ConfinedTrusted => Some("trust"),
            Self::HostAccountTrusted => Some("unsandboxed"),
        }
    }

    /// Truthful human-readable isolation summary for status/welcome/diagnostic views.
    pub fn isolation_summary(self, sandbox_backend: Option<&str>) -> String {
        match self {
            Self::Governed => match sandbox_backend {
                Some(backend) => format!("governed (OS sandbox: {backend})"),
                None => "governed (OS sandbox)".to_string(),
            },
            Self::ConfinedSmart => match sandbox_backend {
                Some(backend) => format!("confined/smart (OS sandbox active: {backend})"),
                None => "confined/smart (OS sandbox active)".to_string(),
            },
            Self::ConfinedTrusted => match sandbox_backend {
                Some(backend) => format!("confined/trusted (OS sandbox active: {backend})"),
                None => "confined/trusted (OS sandbox active)".to_string(),
            },
            Self::HostAccountTrusted => {
                "unsandboxed host-account (OS sandbox disabled; no tamperproof same-account boundary)"
                    .to_string()
            }
        }
    }

    /// Structured telemetry/audit fields describing this posture and its honest boundary.
    pub fn audit_metadata(self, sandbox_backend: Option<&str>) -> serde_json::Value {
        let reported_backend = match self {
            Self::HostAccountTrusted => "unsandboxed-host-account",
            Self::Governed | Self::ConfinedSmart | Self::ConfinedTrusted => {
                sandbox_backend.unwrap_or("os-sandbox")
            }
        };
        json!({
            "trust_posture": self.as_str(),
            "confined": self.uses_os_sandbox(),
            "tamperproof_control_boundary": self.offers_tamperproof_confinement(),
            "sandbox_backend": reported_backend,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_flags_resolve_all_valid_combinations_and_reject_invalid_combinations() {
        assert_eq!(
            TrustPosture::from_cli_flags(false, false).unwrap(),
            TrustPosture::Governed
        );
        assert_eq!(
            TrustPosture::from_cli_flags(true, false).unwrap(),
            TrustPosture::ConfinedTrusted
        );
        assert_eq!(
            TrustPosture::from_cli_flags(true, true).unwrap(),
            TrustPosture::HostAccountTrusted
        );
        assert_eq!(
            TrustPosture::from_cli_flags(false, true).unwrap_err(),
            TrustPostureError::UnsafeHostAccountWithoutTrust
        );

        assert_eq!(
            TrustPosture::from_cli_posture(false, false, false).unwrap(),
            TrustPosture::Governed
        );
        assert_eq!(
            TrustPosture::from_cli_posture(true, false, false).unwrap(),
            TrustPosture::ConfinedSmart
        );
        assert_eq!(
            TrustPosture::from_cli_posture(false, true, false).unwrap(),
            TrustPosture::ConfinedTrusted
        );
        assert_eq!(
            TrustPosture::from_cli_posture(false, true, true).unwrap(),
            TrustPosture::HostAccountTrusted
        );
        assert_eq!(
            TrustPosture::from_cli_posture(false, false, true).unwrap_err(),
            TrustPostureError::UnsafeHostAccountWithoutTrust
        );
        assert_eq!(
            TrustPosture::from_cli_posture(true, true, false).unwrap_err(),
            TrustPostureError::SmartConflictsWithTrust
        );
        assert_eq!(
            TrustPosture::from_cli_posture(true, true, true).unwrap_err(),
            TrustPostureError::SmartConflictsWithTrust
        );
        assert_eq!(
            TrustPosture::from_cli_posture(true, false, true).unwrap_err(),
            TrustPostureError::SmartConflictsWithUnsafeHostAccount
        );
    }

    #[test]
    fn internal_tokens_round_trip_for_all_postures() {
        for (token, posture) in [
            ("governed", TrustPosture::Governed),
            ("confined_smart", TrustPosture::ConfinedSmart),
            ("confined_trusted", TrustPosture::ConfinedTrusted),
            ("host_account_trusted", TrustPosture::HostAccountTrusted),
        ] {
            assert_eq!(TrustPosture::from_internal_token(token).unwrap(), posture);
            assert_eq!(posture.as_str(), token);
        }
        assert!(TrustPosture::from_internal_token("smart").is_err());
    }

    #[test]
    fn child_posture_allows_equal_or_narrower_and_rejects_broader() {
        assert_eq!(
            TrustPosture::resolve_child(TrustPosture::ConfinedTrusted, None).unwrap(),
            TrustPosture::ConfinedTrusted
        );
        assert_eq!(
            TrustPosture::resolve_child(TrustPosture::ConfinedSmart, None).unwrap(),
            TrustPosture::ConfinedSmart
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::HostAccountTrusted,
                Some(TrustPosture::ConfinedTrusted),
            )
            .unwrap(),
            TrustPosture::ConfinedTrusted
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::ConfinedTrusted,
                Some(TrustPosture::ConfinedSmart),
            )
            .unwrap(),
            TrustPosture::ConfinedSmart
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::ConfinedSmart,
                Some(TrustPosture::ConfinedSmart),
            )
            .unwrap(),
            TrustPosture::ConfinedSmart
        );
        assert_eq!(
            TrustPosture::resolve_child(TrustPosture::ConfinedSmart, Some(TrustPosture::Governed),)
                .unwrap(),
            TrustPosture::Governed
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::ConfinedTrusted,
                Some(TrustPosture::Governed),
            )
            .unwrap(),
            TrustPosture::Governed
        );
        assert_eq!(
            TrustPosture::resolve_child(TrustPosture::Governed, Some(TrustPosture::ConfinedSmart),)
                .unwrap_err(),
            TrustPostureError::ChildBroaderThanParent {
                requested: "confined_smart",
                parent: "governed",
            }
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::ConfinedSmart,
                Some(TrustPosture::ConfinedTrusted),
            )
            .unwrap_err(),
            TrustPostureError::ChildBroaderThanParent {
                requested: "confined_trusted",
                parent: "confined_smart",
            }
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::Governed,
                Some(TrustPosture::ConfinedTrusted),
            )
            .unwrap_err(),
            TrustPostureError::ChildBroaderThanParent {
                requested: "confined_trusted",
                parent: "governed",
            }
        );
        assert_eq!(
            TrustPosture::resolve_child(
                TrustPosture::ConfinedTrusted,
                Some(TrustPosture::HostAccountTrusted),
            )
            .unwrap_err(),
            TrustPostureError::ChildBroaderThanParent {
                requested: "host_account_trusted",
                parent: "confined_trusted",
            }
        );
    }

    #[test]
    fn postures_report_badges_confinement_and_audit_metadata_truthfully() {
        assert_eq!(TrustPosture::Governed.tui_badge_label(), None);
        assert_eq!(TrustPosture::ConfinedSmart.tui_badge_label(), Some("smart"));
        assert_eq!(
            TrustPosture::ConfinedTrusted.tui_badge_label(),
            Some("trust")
        );
        assert_eq!(
            TrustPosture::HostAccountTrusted.tui_badge_label(),
            Some("unsandboxed")
        );

        assert!(TrustPosture::ConfinedSmart.uses_os_sandbox());
        assert!(TrustPosture::ConfinedSmart.enforces_host_mount_bounds());
        assert!(TrustPosture::ConfinedSmart.offers_tamperproof_confinement());
        assert!(!TrustPosture::ConfinedSmart.bypasses_action_approvals());
        assert!(!TrustPosture::ConfinedSmart.is_unsandboxed_host_account());
        let smart_summary = TrustPosture::ConfinedSmart.isolation_summary(Some("seatbelt"));
        assert!(smart_summary.contains("confined/smart"));
        assert!(smart_summary.contains("seatbelt"));
        let smart_audit = TrustPosture::ConfinedSmart.audit_metadata(Some("seatbelt"));
        assert_eq!(smart_audit["trust_posture"], "confined_smart");
        assert_eq!(smart_audit["confined"], true);
        assert_eq!(smart_audit["tamperproof_control_boundary"], true);
        assert_eq!(smart_audit["sandbox_backend"], "seatbelt");

        assert!(TrustPosture::ConfinedTrusted.uses_os_sandbox());
        assert!(TrustPosture::ConfinedTrusted.enforces_host_mount_bounds());
        assert!(TrustPosture::ConfinedTrusted.bypasses_action_approvals());

        assert!(!TrustPosture::HostAccountTrusted.uses_os_sandbox());
        assert!(!TrustPosture::HostAccountTrusted.offers_tamperproof_confinement());
        let summary = TrustPosture::HostAccountTrusted.isolation_summary(Some("seatbelt"));
        assert!(summary.contains("unsandboxed host-account"));
        assert!(!summary.contains("seatbelt"));

        let audit = TrustPosture::HostAccountTrusted.audit_metadata(Some("seatbelt"));
        assert_eq!(audit["confined"], false);
        assert_eq!(audit["tamperproof_control_boundary"], false);
        assert_eq!(audit["sandbox_backend"], "unsandboxed-host-account");
    }

    #[test]
    fn repository_config_and_agent_definition_reject_trust_posture_fields() {
        for malicious_toml in [
            "smart = true\n",
            "trust = true\n",
            "trust_posture = \"confined_smart\"\n",
            "trust_posture = \"confined_trusted\"\n",
            "trust_posture = \"host_account_trusted\"\n",
            "unsafe_host_account = true\n",
        ] {
            let parsed = toml::from_str::<crate::Config>(malicious_toml);
            assert!(
                parsed.is_err(),
                "Config must reject non-authoritative trust field `{malicious_toml}`"
            );
        }
    }

    #[test]
    fn runtime_agent_process_config_ignores_default_and_narrows_children_only() {
        let mut parent = crate::runtime::AgentProcessConfig::default();
        assert_eq!(parent.trust_posture, TrustPosture::Governed);
        parent.trust_posture = TrustPosture::ConfinedTrusted;

        let spec = alan_agent_protocol::SpawnSpec {
            target: alan_agent_protocol::SpawnTarget::DefinitionDescriptor {
                descriptor: "agent-definition".to_string(),
            },
            launch: alan_agent_protocol::SpawnLaunchInputs::default(),
            handles: Vec::new(),
            host_mounts: Vec::new(),
            runtime_overrides: alan_agent_protocol::SpawnRuntimeOverrides::default(),
            delegated: None,
        };
        let req = alan_agent_protocol::AgentExecutableRequest {
            spawn: spec,
            initial_task: "check child posture".to_string(),
            resume: None,
            recovery_rollout: None,
            inherited_resume: false,
            async_workstream_lease: None,
        };
        let child = parent.child_for_spawn(&req);
        assert_eq!(child.trust_posture, TrustPosture::ConfinedTrusted);
        assert!(
            child
                .trust_posture
                .is_equal_or_narrower_than(parent.trust_posture)
        );
    }
}
