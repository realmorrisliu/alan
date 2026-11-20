//! In-TUI `/login` provider wizard and Host connection control contract.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use alan_agent_protocol::InputIntent;
use anyhow::Result;
use tokio::sync::mpsc;

use super::app::FileBackedEvent;
pub(in crate::file_backed) use super::login_flow::LoginFlowState;

/// Interaction style hint for a provider listed in `/login`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginMethod {
    /// Browser OAuth, device code, or local probe that may emit progress/URL updates while waiting.
    BrowserOrAuto,
    /// Secret / API key entry (with optional custom base URL prompt).
    ApiKey {
        /// Whether this provider accepts a custom base URL override.
        allow_base_url: bool,
    },
    /// Cloud auth (OAuth / ADC / gcloud) that may return a secondary selection list (e.g. GCP projects).
    CloudProject,
}

/// One selectable provider choice presented in the `/login` wizard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginProviderChoice {
    /// Canonical provider id passed back in [`LoginSubmission::provider_id`]
    /// (for example `"chatgpt"`, `"openrouter"`, `"vertex"`, `"magpie"`, `"openai"`).
    pub id: String,
    /// Human-readable display label (for example `"ChatGPT (Plus / Pro OAuth)"`).
    pub label: String,
    /// Short auth method or status summary shown next to the label.
    pub detail: String,
    /// Suggested default profile id (for example `"chatgpt-main"`).
    pub default_profile: String,
    /// Interaction hint for the TUI.
    pub method: LoginMethod,
}

/// Secondary selectable item returned by [`LoginResult::NeedsSelection`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginSelectOption {
    /// Canonical value passed back in [`LoginSubmission::selection`].
    pub id: String,
    /// Human-readable option name.
    pub label: String,
    /// Optional secondary detail (for example project id or region).
    pub detail: Option<String>,
}

/// Which text field is requested by [`LoginResult::NeedsInput`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginInputField {
    /// API key or bearer secret (masked when `masked == true`).
    Secret,
    /// Custom provider base URL (for example `http://127.0.0.1:3425/v1`).
    BaseUrl,
    /// Cloud project id (manual fallback when no active projects are listed).
    ProjectId,
    /// Manual OAuth authorization code or callback URL paste.
    AuthCode,
    /// Explicit connection profile id.
    ProfileId,
}

/// Live progress event sent from a running [`LoginControl::Submit`] future to the TUI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginProgress {
    /// Short status line while waiting (for example `"Probing local Magpie router…"`).
    Status(String),
    /// Verification URL (and optional device/user code) displayed while awaiting browser OAuth.
    BrowserPrompt {
        /// Verification or authorization URL the user can open.
        url: String,
        /// Optional device/user confirmation code.
        user_code: Option<String>,
        /// Optional explanatory hint shown below the URL.
        hint: Option<String>,
    },
}

/// Non-blocking sender passed inside [`LoginSubmission`] for reporting live OAuth URLs/status.
#[derive(Clone, Debug)]
pub struct LoginProgressSender {
    tx: mpsc::UnboundedSender<LoginProgress>,
}

impl LoginProgressSender {
    /// Create a sender/receiver pair for streaming login progress to the TUI.
    pub fn channel() -> (Self, mpsc::UnboundedReceiver<LoginProgress>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    /// Emit a [`LoginProgress`] update. Returns `false` if the user cancelled `/login`.
    pub fn send(&self, progress: LoginProgress) -> bool {
        self.tx.send(progress).is_ok()
    }

    /// Emit a status line while waiting.
    pub fn status(&self, message: impl Into<String>) -> bool {
        self.send(LoginProgress::Status(message.into()))
    }

    /// Emit a browser URL (and optional user code / hint) while awaiting browser OAuth.
    pub fn browser_prompt(
        &self,
        url: impl Into<String>,
        user_code: Option<String>,
        hint: Option<String>,
    ) -> bool {
        self.send(LoginProgress::BrowserPrompt {
            url: url.into(),
            user_code,
            hint,
        })
    }
}

/// Accumulated state for one multi-step `/login` submission.
#[derive(Clone)]
pub struct LoginSubmission {
    /// Selected provider id (or explicit `/login <provider>` argument).
    pub provider_id: String,
    /// Optional explicit profile id.
    pub profile_id: Option<String>,
    /// Optional secret/API key entered by the user (redacted in `Debug`).
    pub secret: Option<String>,
    /// Optional custom endpoint base URL.
    pub base_url: Option<String>,
    /// Most recent selection id from [`LoginResult::NeedsSelection`].
    pub selection: Option<String>,
    /// Ordered history of all [`LoginResult::NeedsSelection`] ids chosen in this attempt.
    pub selections: Vec<String>,
    /// Optional manual OAuth code / callback URL (redacted in `Debug`).
    pub auth_code: Option<String>,
    /// Optional manual cloud project id.
    pub project_id: Option<String>,
    /// Live progress channel while waiting.
    pub progress: LoginProgressSender,
}

impl LoginSubmission {
    pub(in crate::file_backed) fn new(
        provider_id: String,
        profile_id: Option<String>,
        progress: LoginProgressSender,
    ) -> Self {
        Self {
            provider_id,
            profile_id,
            secret: None,
            base_url: None,
            selection: None,
            selections: Vec::new(),
            auth_code: None,
            project_id: None,
            progress,
        }
    }
}

impl std::fmt::Debug for LoginSubmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoginSubmission")
            .field("provider_id", &self.provider_id)
            .field("profile_id", &self.profile_id)
            .field("secret", &self.secret.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("selection", &self.selection)
            .field("selections", &self.selections)
            .field("auth_code", &self.auth_code.as_ref().map(|_| "<redacted>"))
            .field("project_id", &self.project_id)
            .finish()
    }
}

/// Control request sent from the TUI `/login` flow to the Host adapter.
#[derive(Clone, Debug)]
pub enum LoginControl {
    /// Return selectable providers and their current connection state.
    ListProviders,
    /// Begin or continue interactive login for the selected provider.
    Submit(LoginSubmission),
}

/// Result returned by [`LoginControlHandler`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoginResult {
    /// Response to [`LoginControl::ListProviders`].
    Providers(Vec<LoginProviderChoice>),
    /// Ask the user to pick from a secondary option list (e.g. active GCP projects).
    NeedsSelection {
        /// Prompt title shown above the selector.
        title: String,
        /// Helper guidance line.
        prompt: String,
        /// Selectable options.
        options: Vec<LoginSelectOption>,
    },
    /// Ask the user to type/paste an input field inside the TUI.
    NeedsInput {
        /// Which field on [`LoginSubmission`] to populate.
        field: LoginInputField,
        /// Title shown in the prompt header.
        title: String,
        /// Instructions shown to the user.
        prompt: String,
        /// Whether to mask entered characters in the TUI.
        masked: bool,
        /// Whether submitting an empty value is allowed.
        allow_empty: bool,
        /// Optional default text pre-filled when unmasked.
        default_value: Option<String>,
    },
    /// Login succeeded, connection metadata/secrets persisted, and live host reloaded.
    Connected {
        /// Saved connection profile identifier.
        profile: String,
        /// Connected provider name.
        provider: String,
        /// Effective default model after connecting, if resolved.
        model: Option<String>,
        /// Optional confirmation summary from the Host.
        message: Option<String>,
    },
    /// Non-fatal auth/validation failure presented cleanly in the TUI.
    Failed {
        /// Human-readable error message.
        message: String,
    },
}

/// Future returned by [`LoginControlHandler`].
pub type LoginControlFuture = Pin<Box<dyn Future<Output = Result<LoginResult>> + Send + 'static>>;

/// Host-local login/connection configuration handler supplied by the CLI to the TUI.
pub type LoginControlHandler =
    Arc<dyn Fn(LoginControl) -> LoginControlFuture + Send + Sync + 'static>;

pub(in crate::file_backed) fn spawn_list_providers(
    handler: LoginControlHandler,
    tx: mpsc::Sender<FileBackedEvent>,
    generation: u64,
) -> tokio::task::AbortHandle {
    let handle = tokio::spawn(async move {
        let result = handler(LoginControl::ListProviders)
            .await
            .map_err(|err| format!("{err:#}"));
        let _ = tx
            .send(FileBackedEvent::LoginProvidersLoaded { generation, result })
            .await;
    });
    handle.abort_handle()
}

pub(in crate::file_backed) fn spawn_login_submit(
    handler: LoginControlHandler,
    tx: mpsc::Sender<FileBackedEvent>,
    generation: u64,
    submission: LoginSubmission,
    progress_rx: Option<mpsc::UnboundedReceiver<LoginProgress>>,
) -> tokio::task::AbortHandle {
    let handle = tokio::spawn(async move {
        let forward = progress_rx.map(|mut rx| {
            let progress_tx = tx.clone();
            tokio::spawn(async move {
                while let Some(progress) = rx.recv().await {
                    if progress_tx
                        .send(FileBackedEvent::LoginProgress {
                            generation,
                            progress,
                        })
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            })
        });
        let result = handler(LoginControl::Submit(submission))
            .await
            .map_err(|err| format!("{err:#}"));
        if let Some(task) = forward {
            task.abort();
        }
        let _ = tx
            .send(FileBackedEvent::LoginStepCompleted { generation, result })
            .await;
    });
    handle.abort_handle()
}

pub(in crate::file_backed) fn spawn_post_login_model_sync(
    transport: alan_ap::InProcessTransport,
    owner: String,
    model: Option<String>,
    model_reads: super::observation_io::Request,
) {
    tokio::spawn(async move {
        if let Some(model) = model.filter(|m| !m.trim().is_empty())
            && !owner.is_empty()
            && owner != "/agent/root"
        {
            let shell = alan_shell::Shell::new(transport);
            let id = alan_agent_protocol::UserInputRecord::new(
                InputIntent::Agent,
                alan_agent_protocol::InputMode::FollowUp,
                "",
            )
            .submission_id;
            let _ = super::file_surface::write_machine_ctl(
                &shell,
                &owner,
                &format!("select-model {id} {model}"),
            )
            .await;
        }
        if !owner.is_empty() {
            super::observation_io::request(&model_reads, owner);
        }
    });
}
