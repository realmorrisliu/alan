//! Composes `/mnt/github/events` with `alan_routefs::RouteFs` (`/mnt/route`)
//! enforcing control-plane allowlists and loop guards.
//!
//! Security and idempotency invariants:
//! - Trigger eligibility inspects ONLY control-plane [`TriggerAllowlist`] and
//!   structured [`GithubEvent`] fields (`repo`, `actor`, `labels`, `kind`, `branch`),
//!   never `body.md` or `comments` prose.
//! - Duplicate `event_id` values are skipped.
//! - Self-actors and self-branch push/sync cycles are suppressed before allowlist matching.
//! - Concurrent active runs are coalesced by `(repo, number, head_sha)` and bounded
//!   by `max_concurrent`.
//! - Exceeding per-issue `max_retries` escalates to `/mnt/route/ports/human-review`.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use alan_ap::{ErrorCode, Fid, FileKind, InProcessTransport, Offset, OpenMode, Request};
use alan_routefs::{RouteFs, RuleSpec};
use alan_shell::Shell;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::backend::{GithubEvent, RepoSlug, validate_label};

/// Default `routefs` port receiving bounded worker jobs.
pub const WORKER_PORT: &str = "worker";
/// Default `routefs` port receiving retry-exhausted / human-gated escalations.
pub const HUMAN_REVIEW_PORT: &str = "human-review";

const RULE_HUMAN_REVIEW: &str = "10-github-human-review";
const RULE_WORKER: &str = "20-github-worker";
const MSG_TYPE_WORK: &str = "github.work";
const MSG_TYPE_HUMAN_REVIEW: &str = "github.human_review";

const MAX_SEEN_EVENT_IDS: usize = 10_000;
const MAX_PENDING_QUEUE: usize = 128;
const MAX_CONCURRENT_CAP: usize = 64;
const MAX_RETRIES_CAP: u32 = 20;

static DISPATCH_FID: AtomicU64 = AtomicU64::new(1 << 60);

/// Control-plane trigger allowlist and loop-guard configuration.
///
/// Always configured by the operator/control-plane (never parsed from GitHub
/// issue bodies, titles, or comments).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerAllowlist {
    pub allowed_repos: Vec<String>,
    pub allowed_actors: Vec<String>,
    pub required_labels: Vec<String>,
    #[serde(default)]
    pub ignored_actors: Vec<String>,
    #[serde(default)]
    pub ignored_branch_prefixes: Vec<String>,
    pub max_concurrent: usize,
    pub max_retries: u32,
}

impl TriggerAllowlist {
    /// Validate control-plane allowlist invariants (fails closed on empty/unsafe bounds).
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.allowed_repos.is_empty(),
            "TriggerAllowlist requires at least one allowlisted repo"
        );
        for repo in &self.allowed_repos {
            RepoSlug::parse(repo)
                .map_err(|_| anyhow::anyhow!("invalid allowlisted repo `{repo}`"))?;
        }
        ensure!(
            !self.allowed_actors.is_empty(),
            "TriggerAllowlist requires at least one allowlisted actor"
        );
        for actor in &self.allowed_actors {
            ensure!(valid_actor(actor), "invalid allowlisted actor `{actor}`");
        }
        ensure!(
            !self.required_labels.is_empty(),
            "TriggerAllowlist requires at least one required label (e.g. `alan:auto`)"
        );
        for label in &self.required_labels {
            validate_label(label)
                .map_err(|_| anyhow::anyhow!("invalid required label `{label}`"))?;
        }
        for ignored in &self.ignored_actors {
            ensure!(valid_actor(ignored), "invalid ignored actor `{ignored}`");
        }
        ensure!(
            (1..=MAX_CONCURRENT_CAP).contains(&self.max_concurrent),
            "max_concurrent must be in 1..={MAX_CONCURRENT_CAP}"
        );
        ensure!(
            (1..=MAX_RETRIES_CAP).contains(&self.max_retries),
            "max_retries must be in 1..={MAX_RETRIES_CAP}"
        );
        Ok(())
    }

    fn is_ignored_actor(&self, actor: &str) -> bool {
        let trimmed = actor.trim();
        self.ignored_actors
            .iter()
            .any(|ignored| ignored.trim().eq_ignore_ascii_case(trimmed))
    }

    fn is_repo_allowed(&self, repo: &str) -> bool {
        let Ok(candidate) = RepoSlug::parse(repo) else {
            return false;
        };
        let normalized = candidate.as_slug();
        self.allowed_repos.iter().any(|allowed| {
            RepoSlug::parse(allowed)
                .map(|slug| slug.as_slug().eq_ignore_ascii_case(&normalized))
                .unwrap_or(false)
        })
    }

    fn is_actor_allowed(&self, actor: &str) -> bool {
        let trimmed = actor.trim();
        !trimmed.is_empty()
            && self
                .allowed_actors
                .iter()
                .any(|allowed| allowed.trim().eq_ignore_ascii_case(trimmed))
    }

    fn has_required_labels(&self, labels: &[String]) -> bool {
        self.required_labels
            .iter()
            .all(|required| labels.iter().any(|present| present == required))
    }

    fn is_self_branch_loop(&self, event: &GithubEvent) -> bool {
        let Some(branch) = event.branch.as_deref() else {
            return false;
        };
        let matches_prefix = self
            .ignored_branch_prefixes
            .iter()
            .any(|prefix| !prefix.is_empty() && branch.starts_with(prefix));
        if !matches_prefix {
            return false;
        }
        matches!(
            event.kind.as_str(),
            "push" | "pull_synchronized" | "synchronize" | "commit" | "pull_opened"
        )
    }
}

fn valid_actor(actor: &str) -> bool {
    let trimmed = actor.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 64
        && !trimmed.chars().any(char::is_whitespace)
        && !trimmed.contains('\0')
}

/// Canonical issue/PR key for retry accounting and human-review escalation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IssueKey {
    pub repo: String,
    pub number: u64,
}

impl IssueKey {
    pub fn new(repo: &str, number: u64) -> Self {
        Self {
            repo: repo.to_ascii_lowercase(),
            number,
        }
    }
}

/// Active-run coalescing key `(repo, number, head_sha)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CoalesceKey {
    pub repo: String,
    pub number: u64,
    pub head_sha: Option<String>,
}

impl CoalesceKey {
    pub fn new(repo: &str, number: u64, head_sha: Option<&str>) -> Self {
        Self {
            repo: repo.to_ascii_lowercase(),
            number,
            head_sha: head_sha
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string),
        }
    }
}

/// Reason an incoming GitHub event did not spawn a worker run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    MalformedEvent,
    DuplicateEventId,
    SelfActor,
    SelfBranchLoop,
    RepoNotAllowed,
    ActorNotAllowed,
    MissingRequiredLabel,
    MissingTargetNumber,
    CoalescedActiveRun,
    AlreadyEscalated,
}

/// Outcome returned for each evaluated GitHub event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    DispatchedWorker {
        event_id: String,
        coalesce_key: CoalesceKey,
        attempt: u32,
    },
    QueuedConcurrencyCap {
        event_id: String,
        coalesce_key: CoalesceKey,
    },
    EscalatedHumanReview {
        event_id: String,
        issue_key: IssueKey,
        attempts: u32,
        reason: String,
    },
    Skipped {
        event_id: String,
        reason: SkipReason,
    },
}

/// Observable counters for dispatcher loop guards and routing outcomes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DispatchStats {
    pub events_processed: u64,
    pub dispatched_worker: u64,
    pub escalated_human_review: u64,
    pub deduplicated: u64,
    pub self_suppressed: u64,
    pub allowlist_rejected: u64,
    pub coalesced: u64,
    pub queued_concurrency: u64,
    pub active_runs: usize,
    pub pending_queue: usize,
}

#[derive(Debug, Clone)]
struct QueuedWork {
    event: GithubEvent,
    issue_key: IssueKey,
    coalesce_key: CoalesceKey,
}

/// Construct a fresh `RouteFs` transport ready to publish on `/srv/route` and mount at `/mnt/route`.
pub fn new_routefs_transport() -> InProcessTransport {
    InProcessTransport::new(Arc::new(RouteFs::new()))
}

/// Install the default inspectable plain-text routing rules directly into a [`RouteFs`].
pub async fn install_github_dispatch_rules(routefs: &RouteFs) -> Result<(), ErrorCode> {
    routefs
        .install_rule(
            RULE_HUMAN_REVIEW,
            RuleSpec::for_type(MSG_TYPE_HUMAN_REVIEW, HUMAN_REVIEW_PORT)
                .with_reason("escalate retry-exhausted or human-gated github items"),
        )
        .await?;
    routefs
        .install_rule(
            RULE_WORKER,
            RuleSpec::for_type(MSG_TYPE_WORK, WORKER_PORT)
                .with_reason("dispatch allowlisted github work items to bounded worker"),
        )
        .await?;
    Ok(())
}

/// Bounded dispatcher reading `/mnt/github/events` and routing via `/mnt/route`.
pub struct GithubEventDispatcher {
    transport: InProcessTransport,
    shell: Shell,
    github_events_path: String,
    route_base_path: String,
    allowlist: TriggerAllowlist,
    rules_installed: bool,
    event_offset: Offset,
    line_buf: Vec<u8>,
    seen_event_ids: HashSet<String>,
    seen_order: VecDeque<String>,
    active_runs: HashMap<CoalesceKey, u32>,
    pending_queue: VecDeque<QueuedWork>,
    attempts_by_issue: HashMap<IssueKey, u32>,
    escalated_issues: HashSet<IssueKey>,
    stats: DispatchStats,
}

impl GithubEventDispatcher {
    /// Create a dispatcher operating over a mounted namespace (`transport`) using
    /// default `/mnt/github/events` and `/mnt/route` paths.
    pub fn new(transport: InProcessTransport, allowlist: TriggerAllowlist) -> Result<Self> {
        Self::with_paths(transport, "/mnt/github/events", "/mnt/route", allowlist)
    }

    /// Create a dispatcher with explicit namespace paths for GitHub events and `routefs`.
    pub fn with_paths(
        transport: InProcessTransport,
        github_events_path: impl Into<String>,
        route_base_path: impl Into<String>,
        allowlist: TriggerAllowlist,
    ) -> Result<Self> {
        allowlist.validate()?;
        let shell = Shell::new(transport.clone());
        Ok(Self {
            transport,
            shell,
            github_events_path: github_events_path.into(),
            route_base_path: route_base_path.into().trim_end_matches('/').to_string(),
            allowlist,
            rules_installed: false,
            event_offset: 0,
            line_buf: Vec::new(),
            seen_event_ids: HashSet::new(),
            seen_order: VecDeque::new(),
            active_runs: HashMap::new(),
            pending_queue: VecDeque::new(),
            attempts_by_issue: HashMap::new(),
            escalated_issues: HashSet::new(),
            stats: DispatchStats::default(),
        })
    }

    /// Return current dispatch statistics and active/pending counts.
    pub fn stats(&self) -> DispatchStats {
        let mut stats = self.stats.clone();
        stats.active_runs = self.active_runs.len();
        stats.pending_queue = self.pending_queue.len();
        stats
    }

    /// Ensure plain-text routing rules exist under `/mnt/route/rules/` via aP `create`/`write`/`clunk`.
    pub async fn ensure_route_rules(&mut self) -> Result<()> {
        if self.rules_installed {
            return Ok(());
        }
        self.ensure_rule_file(
            RULE_HUMAN_REVIEW,
            &RuleSpec::for_type(MSG_TYPE_HUMAN_REVIEW, HUMAN_REVIEW_PORT)
                .with_reason("escalate retry-exhausted or human-gated github items"),
        )
        .await?;
        self.ensure_rule_file(
            RULE_WORKER,
            &RuleSpec::for_type(MSG_TYPE_WORK, WORKER_PORT)
                .with_reason("dispatch allowlisted github work items to bounded worker"),
        )
        .await?;
        self.rules_installed = true;
        Ok(())
    }

    async fn ensure_rule_file(&self, name: &str, spec: &RuleSpec) -> Result<()> {
        let full_path = format!("{}/rules/{name}", self.route_base_path);
        if self.shell.stat(&full_path).await.is_ok() {
            return Ok(());
        }
        let rules_dir_parts: Vec<String> = format!("{}/rules", self.route_base_path)
            .split('/')
            .filter(|part| !part.is_empty())
            .map(ToString::to_string)
            .collect();
        let dir_fid = Fid(DISPATCH_FID.fetch_add(1, Ordering::Relaxed));
        let rule_fid = Fid(DISPATCH_FID.fetch_add(1, Ordering::Relaxed));
        self.transport
            .call(Request::Walk {
                fid: Fid::ROOT,
                newfid: dir_fid,
                names: rules_dir_parts,
            })
            .await
            .map_err(|code| anyhow::anyhow!("walk routefs rules dir failed: {code:?}"))?;

        let result = async {
            self.transport
                .call(Request::Create {
                    fid: dir_fid,
                    newfid: rule_fid,
                    name: name.to_string(),
                    kind: FileKind::File,
                })
                .await
                .map_err(|code| anyhow::anyhow!("create routefs rule `{name}` failed: {code:?}"))?;
            let commit_res = async {
                self.transport
                    .call(Request::Open {
                        fid: rule_fid,
                        mode: OpenMode::Write,
                    })
                    .await
                    .map_err(|code| {
                        anyhow::anyhow!("open routefs rule `{name}` failed: {code:?}")
                    })?;
                let bytes = serde_json::to_vec(spec)?;
                self.transport
                    .call(Request::Write {
                        fid: rule_fid,
                        offset: 0,
                        data: bytes,
                    })
                    .await
                    .map_err(|code| {
                        anyhow::anyhow!("write routefs rule `{name}` failed: {code:?}")
                    })?;
                self.transport
                    .call(Request::Clunk { fid: rule_fid })
                    .await
                    .map_err(|code| {
                        anyhow::anyhow!("commit routefs rule `{name}` failed: {code:?}")
                    })?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            if commit_res.is_err() {
                let _ = self.transport.call(Request::Clunk { fid: rule_fid }).await;
            }
            commit_res
        }
        .await;
        let _ = self.transport.call(Request::Clunk { fid: dir_fid }).await;
        result
    }

    /// Non-blockingly consume all currently retained bytes from `/mnt/github/events`
    /// starting after the previous cursor offset, routing complete NDJSON events.
    pub async fn process_available_events(&mut self) -> Result<Vec<DispatchOutcome>> {
        self.ensure_route_rules().await?;
        let mut outcomes = if !self.line_buf.is_empty() {
            self.ingest_bytes(&[]).await?
        } else {
            Vec::new()
        };
        let stat = self
            .shell
            .stat(&self.github_events_path)
            .await
            .map_err(|code| {
                anyhow::anyhow!("stat `{}` failed: {code:?}", self.github_events_path)
            })?;
        if stat.length <= self.event_offset {
            return Ok(outcomes);
        }
        let mut tail = self
            .shell
            .tail_from(&self.github_events_path, self.event_offset)
            .await
            .map_err(|code| {
                anyhow::anyhow!("tail `{}` failed: {code:?}", self.github_events_path)
            })?;
        while self.event_offset < stat.length {
            let want = (stat.length - self.event_offset).min(4096) as u32;
            match tail.read(want).await {
                Ok(chunk) if !chunk.is_empty() => {
                    self.event_offset += chunk.len() as u64;
                    match self.ingest_bytes(&chunk).await {
                        Ok(mut batch) => outcomes.append(&mut batch),
                        Err(err) => {
                            let _ = tail.close().await;
                            return Err(err);
                        }
                    }
                }
                Ok(_) => break,
                Err(code) => {
                    let _ = tail.close().await;
                    bail!("read `{}` failed: {code:?}", self.github_events_path);
                }
            }
        }
        let _ = tail.close().await;
        Ok(outcomes)
    }

    /// Tail `/mnt/github/events` reactively until `shutdown` fires, exiting cleanly.
    pub async fn tail_events_until(
        &mut self,
        mut shutdown: oneshot::Receiver<()>,
    ) -> Result<Vec<DispatchOutcome>> {
        self.ensure_route_rules().await?;
        let mut outcomes = if !self.line_buf.is_empty() {
            self.ingest_bytes(&[]).await?
        } else {
            Vec::new()
        };
        let mut tail = self
            .shell
            .tail_from(&self.github_events_path, self.event_offset)
            .await
            .map_err(|code| {
                anyhow::anyhow!("tail `{}` failed: {code:?}", self.github_events_path)
            })?;
        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                result = tail.read(4096) => {
                    match result {
                        Ok(chunk) if chunk.is_empty() => break,
                        Ok(chunk) => {
                            self.event_offset += chunk.len() as u64;
                            match self.ingest_bytes(&chunk).await {
                                Ok(mut batch) => outcomes.append(&mut batch),
                                Err(err) => {
                                    let _ = tail.close().await;
                                    return Err(err);
                                }
                            }
                        }
                        Err(code) => {
                            let _ = tail.close().await;
                            bail!("tail `{}` failed: {code:?}", self.github_events_path);
                        }
                    }
                }
            }
        }
        let _ = tail.close().await;
        Ok(outcomes)
    }

    async fn ingest_bytes(&mut self, bytes: &[u8]) -> Result<Vec<DispatchOutcome>> {
        self.line_buf.extend_from_slice(bytes);
        let mut outcomes = Vec::new();
        while let Some(newline_pos) = self.line_buf.iter().position(|b| *b == b'\n') {
            let line_bytes: Vec<u8> = self.line_buf.drain(..=newline_pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let outcome = match serde_json::from_str::<GithubEvent>(trimmed) {
                Ok(event) => match self.process_event(&event).await {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        self.line_buf.splice(0..0, line_bytes);
                        return Err(err);
                    }
                },
                Err(_) => {
                    self.stats.events_processed += 1;
                    self.stats.allowlist_rejected += 1;
                    DispatchOutcome::Skipped {
                        event_id: String::new(),
                        reason: SkipReason::MalformedEvent,
                    }
                }
            };
            outcomes.push(outcome);
        }
        Ok(outcomes)
    }

    /// Evaluate one [`GithubEvent`] against control-plane allowlists and loop guards,
    /// routing accepted or exhausted items through `/mnt/route/send`.
    pub async fn process_event(&mut self, event: &GithubEvent) -> Result<DispatchOutcome> {
        self.ensure_route_rules().await?;

        let event_id = event.event_id.trim();
        if event_id.is_empty()
            || RepoSlug::parse(&event.repo).is_err()
            || !valid_actor(&event.actor)
        {
            self.stats.events_processed = self.stats.events_processed.saturating_add(1);
            self.stats.allowlist_rejected += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::MalformedEvent,
            });
        }

        if self.seen_event_ids.contains(event_id) {
            self.stats.events_processed = self.stats.events_processed.saturating_add(1);
            self.stats.deduplicated += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::DuplicateEventId,
            });
        }

        let outcome = self.evaluate_and_route_event(event).await?;
        self.record_event_id(event_id);
        self.stats.events_processed = self.stats.events_processed.saturating_add(1);
        Ok(outcome)
    }

    async fn evaluate_and_route_event(&mut self, event: &GithubEvent) -> Result<DispatchOutcome> {
        if self.allowlist.is_ignored_actor(&event.actor) {
            self.stats.self_suppressed += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::SelfActor,
            });
        }
        if self.allowlist.is_self_branch_loop(event) {
            self.stats.self_suppressed += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::SelfBranchLoop,
            });
        }

        if !self.allowlist.is_repo_allowed(&event.repo) {
            self.stats.allowlist_rejected += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::RepoNotAllowed,
            });
        }
        if !self.allowlist.is_actor_allowed(&event.actor) {
            self.stats.allowlist_rejected += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::ActorNotAllowed,
            });
        }
        if !self.allowlist.has_required_labels(&event.labels) {
            self.stats.allowlist_rejected += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::MissingRequiredLabel,
            });
        }
        let Some(number) = event.number.filter(|n| *n > 0) else {
            self.stats.allowlist_rejected += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::MissingTargetNumber,
            });
        };

        let issue_key = IssueKey::new(&event.repo, number);
        if self.escalated_issues.contains(&issue_key) {
            self.stats.coalesced += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::AlreadyEscalated,
            });
        }

        let coalesce_key = CoalesceKey::new(&event.repo, number, event.head_sha.as_deref());
        if self.active_runs.contains_key(&coalesce_key)
            || self
                .pending_queue
                .iter()
                .any(|queued| queued.coalesce_key == coalesce_key)
        {
            self.stats.coalesced += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: event.event_id.clone(),
                reason: SkipReason::CoalescedActiveRun,
            });
        }

        self.dispatch_or_queue(QueuedWork {
            event: event.clone(),
            issue_key,
            coalesce_key,
        })
        .await
    }

    async fn dispatch_or_queue(&mut self, work: QueuedWork) -> Result<DispatchOutcome> {
        if self.escalated_issues.contains(&work.issue_key) {
            self.stats.coalesced += 1;
            return Ok(DispatchOutcome::Skipped {
                event_id: work.event.event_id,
                reason: SkipReason::AlreadyEscalated,
            });
        }
        let prior = self
            .attempts_by_issue
            .get(&work.issue_key)
            .copied()
            .unwrap_or(0);
        let next_attempt = prior.saturating_add(1);
        if next_attempt > self.allowlist.max_retries {
            return self
                .escalate_to_human_review(
                    &work.event,
                    work.issue_key,
                    prior,
                    format!(
                        "retry cap exhausted ({prior}/{})",
                        self.allowlist.max_retries
                    ),
                )
                .await;
        }
        if self.active_runs.len() >= self.allowlist.max_concurrent {
            if self.pending_queue.len() >= MAX_PENDING_QUEUE {
                return self
                    .escalate_to_human_review(
                        &work.event,
                        work.issue_key,
                        prior,
                        "pending worker queue full".to_string(),
                    )
                    .await;
            }
            let event_id = work.event.event_id.clone();
            let coalesce_key = work.coalesce_key.clone();
            self.pending_queue.push_back(work);
            self.stats.queued_concurrency += 1;
            return Ok(DispatchOutcome::QueuedConcurrencyCap {
                event_id,
                coalesce_key,
            });
        }

        self.send_worker_envelope(&work.event, next_attempt).await?;
        self.attempts_by_issue
            .insert(work.issue_key.clone(), next_attempt);
        self.active_runs
            .insert(work.coalesce_key.clone(), next_attempt);
        self.stats.dispatched_worker += 1;
        Ok(DispatchOutcome::DispatchedWorker {
            event_id: work.event.event_id,
            coalesce_key: work.coalesce_key,
            attempt: next_attempt,
        })
    }

    /// Mark an active worker run completed (or failed) and drain queued work into freed slots.
    pub async fn complete_run(&mut self, key: &CoalesceKey) -> Result<Vec<DispatchOutcome>> {
        self.active_runs.remove(key);
        let mut drained = Vec::new();
        while self.active_runs.len() < self.allowlist.max_concurrent {
            let Some(next) = self.pending_queue.pop_front() else {
                break;
            };
            match self.dispatch_or_queue(next.clone()).await {
                Ok(outcome) => drained.push(outcome),
                Err(err) => {
                    self.pending_queue.push_front(next);
                    return Err(err);
                }
            }
        }
        Ok(drained)
    }

    fn record_event_id(&mut self, event_id: &str) -> bool {
        if self.seen_event_ids.contains(event_id) {
            return false;
        }
        if self.seen_order.len() >= MAX_SEEN_EVENT_IDS
            && let Some(oldest) = self.seen_order.pop_front()
        {
            self.seen_event_ids.remove(&oldest);
        }
        let owned = event_id.to_string();
        self.seen_order.push_back(owned.clone());
        self.seen_event_ids.insert(owned);
        true
    }

    async fn send_worker_envelope(&self, event: &GithubEvent, attempt: u32) -> Result<()> {
        let target_kind = event.target_kind.as_deref().unwrap_or("issue");
        let number = event.number.unwrap_or(0);
        let subpath = if target_kind == "pull" {
            format!("pulls/{number}")
        } else {
            format!("issues/{number}")
        };
        let envelope = serde_json::json!({
            "version": 1,
            "type": MSG_TYPE_WORK,
            "content": format!(
                "repo={} kind={} number={} actor={} head_sha={} attempt={}",
                event.repo,
                event.kind,
                number,
                event.actor,
                event.head_sha.as_deref().unwrap_or("none"),
                attempt
            ),
            "event_id": event.event_id,
            "cursor": event.cursor,
            "repo": event.repo,
            "number": number,
            "target_kind": target_kind,
            "actor": event.actor,
            "labels": event.labels,
            "head_sha": event.head_sha,
            "attempt": attempt,
            "untrusted_context_paths": [
                format!("/mnt/github/repos/{}/{subpath}/meta.json", event.repo),
                format!("/mnt/github/repos/{}/{subpath}/body.md", event.repo),
            ]
        });
        let bytes = serde_json::to_vec(&envelope)?;
        self.shell
            .write(&format!("{}/send", self.route_base_path), &bytes)
            .await
            .map_err(|code| anyhow::anyhow!("send worker envelope to routefs failed: {code:?}"))
            .context("route worker dispatch envelope")
    }

    async fn escalate_to_human_review(
        &mut self,
        event: &GithubEvent,
        issue_key: IssueKey,
        attempts: u32,
        reason: String,
    ) -> Result<DispatchOutcome> {
        let envelope = serde_json::json!({
            "version": 1,
            "type": MSG_TYPE_HUMAN_REVIEW,
            "content": format!(
                "escalate repo={} number={} actor={} attempts={} reason={reason}",
                event.repo,
                issue_key.number,
                event.actor,
                attempts
            ),
            "event_id": event.event_id,
            "cursor": event.cursor,
            "repo": event.repo,
            "number": issue_key.number,
            "actor": event.actor,
            "attempts": attempts,
            "reason": reason,
        });
        let bytes = serde_json::to_vec(&envelope)?;
        self.shell
            .write(&format!("{}/send", self.route_base_path), &bytes)
            .await
            .map_err(|code| {
                anyhow::anyhow!("send escalation envelope to routefs failed: {code:?}")
            })?;
        self.escalated_issues.insert(issue_key.clone());
        self.stats.escalated_human_review += 1;
        Ok(DispatchOutcome::EscalatedHumanReview {
            event_id: event.event_id.clone(),
            issue_key,
            attempts,
            reason,
        })
    }
}
