//! Subscription-delegated Agent lane (Issue #110) — run agent inference on the plan the user
//! **already pays for**, instead of demanding a second, metered API key.
//!
//! ## Why this exists
//!
//! `FR-AG-06` makes BYOK mandatory for the Agent lane, and the requirements themselves flag the
//! consequence as a revenue risk ("BYOKハードルによるPro転換率低下"). SHOGUN's target user already
//! pays for Claude Pro/Max or a ChatGPT plan; asking them for a SHOGUN subscription *plus* an API
//! key *plus* per-token billing is charging three times for one job.
//!
//! ## What this is NOT
//!
//! There is no public OAuth by which a third-party app may spend a consumer Claude/ChatGPT
//! subscription. Reimplementing a vendor CLI's authorization flow, or lifting the token it stored,
//! is off the table — it breaks the vendors' terms, gets user accounts banned, and is not a base a
//! notarized paid product can stand on. So this module has hard non-goals, asserted by tests below:
//!
//! 1. It never reads another application's credential store (`~/.claude/.credentials.json`,
//!    `~/.codex/auth.json`, another app's Keychain item, …).
//! 2. It never performs an OAuth flow against a vendor's consumer endpoints.
//! 3. It never persists a subscription credential. SHOGUN holds none to persist.
//!
//! ## What it does instead
//!
//! It **delegates**. The user has already installed and signed into a vendor's own agent CLI; this
//! module runs that CLI as a local subprocess in its documented non-interactive mode and reads the
//! answer off stdout. Authentication stays entirely inside the delegate. SHOGUN observes exactly
//! two things about it: whether the binary exists, and whether a run succeeded.
//!
//! ## Invariants this upholds
//!
//! - **Invariant 5 (key separation).** [`SubscriptionAgentClient`] implements [`AgentClient`] and
//!   *deliberately does not implement* [`BatchClient`](super::BatchClient). Indexing / Dream Cycle /
//!   Morning Brief stay on the Select KK Batch API. Beyond the invariant this is a product
//!   requirement: subscription quotas are windowed, and pushing batch volume through one would burn
//!   the user's own Claude Code down to zero — SHOGUN breaking the user's day job.
//! - **Invariant 3 (traceability).** A delegated run is still egress. Exactly one [`TraceRecord`]
//!   is written per completion on [`Route::LocalAgent`], carrying only a digest and byte length.
//! - **Invariant 7 (secrets).** The prompt travels on **stdin**, never argv: argv is world-readable
//!   via `ps` to every process on the machine, and captured text must not be. The child's
//!   environment is also scrubbed of SHOGUN's own API keys, so a delegate can never silently bill
//!   the user's metered API account when they asked to spend their subscription.

use std::collections::BTreeMap;
use std::time::Duration;

use super::traceability::{Route, TraceRecord, TraceabilitySink};
use super::{redact_secrets, AgentClient, LlmError};

/// How long a `--version` probe may take. Generous for a cold binary on a spinning disk, short
/// enough that detecting three delegates never stalls onboarding.
pub const DETECT_TIMEOUT: Duration = Duration::from_secs(10);

/// How long one delegated completion may take before it is killed. Agent CLIs think for a while;
/// an unbounded wait would hang the caller's thread forever if a delegate blocked on an interactive
/// prompt we failed to suppress.
pub const COMPLETION_TIMEOUT: Duration = Duration::from_secs(120);

/// Environment variables scrubbed from every delegate process.
///
/// If SHOGUN's own BYOK key (or a stray shell export) reaches the child, the delegate authenticates
/// with *that* instead of the user's subscription — the user asked to spend a plan they already pay
/// for and would quietly be charged per token instead. Removing these makes the subscription the
/// only credential available to the child.
pub const SCRUBBED_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "GEMINI_API_KEY",
    "GOOGLE_API_KEY",
    "OPENROUTER_API_KEY",
];

// ---------------------------------------------------------------- delegates

/// A vendor agent CLI SHOGUN can delegate the Agent lane to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Delegate {
    /// Anthropic's Claude Code CLI, signed in with a Claude Pro/Max subscription.
    ClaudeCode,
    /// OpenAI's Codex CLI, signed in with a ChatGPT plan.
    Codex,
    /// Google's Gemini CLI, signed in with a Google account.
    GeminiCli,
}

impl Delegate {
    /// Every delegate, in the order the onboarding UI offers them.
    pub const ALL: [Delegate; 3] = [Delegate::ClaudeCode, Delegate::Codex, Delegate::GeminiCli];

    /// The stable id used in settings JSON and across the Tauri boundary.
    pub fn id(self) -> &'static str {
        match self {
            Delegate::ClaudeCode => "claude-code",
            Delegate::Codex => "codex",
            Delegate::GeminiCli => "gemini-cli",
        }
    }

    /// Parse a settings id back. `None` for anything unknown, so a hand-edited settings file can
    /// never select a delegate that does not exist.
    pub fn parse(id: &str) -> Option<Delegate> {
        Delegate::ALL.into_iter().find(|d| d.id() == id)
    }

    /// Product name, for UI.
    pub fn label(self) -> &'static str {
        match self {
            Delegate::ClaudeCode => "Claude Code",
            Delegate::Codex => "Codex",
            Delegate::GeminiCli => "Gemini CLI",
        }
    }

    /// Whose quota a run is spent against. The UI must say this before the user opts in — "this
    /// runs on your own plan" is the entire proposition, and a rate-limit message later is only
    /// intelligible if the user was told which plan it refers to.
    pub fn plan_label(self) -> &'static str {
        match self {
            Delegate::ClaudeCode => "Claude Pro / Max",
            Delegate::Codex => "ChatGPT Plus / Pro",
            Delegate::GeminiCli => "Google account",
        }
    }

    /// The executable looked up on `PATH`.
    pub fn binary(self) -> &'static str {
        match self {
            Delegate::ClaudeCode => "claude",
            Delegate::Codex => "codex",
            Delegate::GeminiCli => "gemini",
        }
    }

    /// The upstream host the delegate's own traffic reaches. Recorded as the traceability
    /// destination: what the user needs from that screen is *who saw this*, and the answer is the
    /// vendor, not the local binary. The local hop is carried by [`Route::LocalAgent`].
    pub fn upstream_host(self) -> &'static str {
        match self {
            Delegate::ClaudeCode => "api.anthropic.com",
            Delegate::Codex => "chatgpt.com",
            Delegate::GeminiCli => "generativelanguage.googleapis.com",
        }
    }

    /// Traceability `destination`: the upstream host plus the local hop that carried it, so the
    /// viewer can distinguish "SHOGUN sent this with your key" from "your Claude Code sent this on
    /// your plan" without consulting another column.
    pub fn destination(self) -> String {
        format!("{} (via local {} CLI)", self.upstream_host(), self.binary())
    }

    /// Arguments that ask for the version. The cheapest "is it installed" probe there is: no
    /// network, no quota, no auth.
    pub fn version_args(self) -> Vec<String> {
        vec!["--version".to_string()]
    }

    /// Arguments for one non-interactive completion, with the prompt arriving on **stdin**.
    ///
    /// These invocation contracts are the single place a vendor CLI change has to be absorbed. They
    /// use each CLI's documented headless mode; the prompt is deliberately absent from argv (see
    /// the module docs on `ps` visibility).
    pub fn completion_args(self) -> Vec<String> {
        match self {
            // `claude -p` is print/headless mode; with stdin piped it reads the prompt from there.
            Delegate::ClaudeCode => {
                vec!["-p".to_string(), "--output-format".to_string(), "text".to_string()]
            }
            // `codex exec -` is the non-interactive subcommand reading the prompt from stdin.
            Delegate::Codex => vec!["exec".to_string(), "-".to_string()],
            // The Gemini CLI runs non-interactively when its stdin is a pipe.
            Delegate::GeminiCli => Vec::new(),
        }
    }
}

// ---------------------------------------------------------------- command seam

/// One subprocess invocation. A value rather than a call so tests can assert on exactly what
/// *would* have been run — which is how the "never touches a credential file" guarantee below is
/// checked rather than merely asserted in prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Fed to the child's stdin and closed. `None` closes stdin immediately.
    pub stdin: Option<String>,
    /// Environment names to remove from the child (see [`SCRUBBED_ENV`]).
    pub env_remove: Vec<String>,
    pub timeout: Duration,
}

/// A finished subprocess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Exit code, or `None` when the child was signalled (including our own timeout kill).
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// Why a subprocess could not be run to completion. Distinct from a non-zero exit: "the binary is
/// not there" and "the binary ran and refused" need different UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// The program is not on `PATH`.
    NotFound,
    /// The child outlived its timeout and was killed.
    Timeout,
    /// Anything else (spawn failure, pipe error), already redacted.
    Io(String),
}

/// The subprocess seam. The real implementation is [`ProcessRunner`]; tests use [`MockRunner`], so
/// every classification rule in this module is exercised on Linux CI with no vendor CLI installed.
pub trait CommandRunner: Send + Sync {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError>;
}

// ---------------------------------------------------------------- detection

/// What SHOGUN knows about a delegate. Deliberately shallow: presence and, when the user has asked
/// for a live check, whether a real completion went through. Nothing here is derived from reading
/// the delegate's stored credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegateState {
    /// The binary is not on `PATH`.
    NotInstalled,
    /// Installed, but SHOGUN has not run a completion, so sign-in status is unknown. This is the
    /// honest resting state: claiming "ready" from the binary's existence alone would send the user
    /// out of onboarding into a first action that fails.
    Installed { version: String },
    /// A completion succeeded — the delegate is installed *and* signed in.
    Ready { version: String },
    /// Installed but the delegate says it is not signed in.
    NeedsLogin { version: String },
    /// Installed and signed in, but the plan's quota is currently exhausted.
    RateLimited { version: String },
}

impl DelegateState {
    /// Whether the Agent lane can be driven through this delegate right now.
    pub fn is_usable(&self) -> bool {
        matches!(self, DelegateState::Ready { .. })
    }

    /// Whether the binary is present at all (any state but [`DelegateState::NotInstalled`]).
    pub fn is_installed(&self) -> bool {
        !matches!(self, DelegateState::NotInstalled)
    }

    /// A stable tag for the UI and for logs. Never carries the version or any provider text.
    pub fn tag(&self) -> &'static str {
        match self {
            DelegateState::NotInstalled => "not_installed",
            DelegateState::Installed { .. } => "installed",
            DelegateState::Ready { .. } => "ready",
            DelegateState::NeedsLogin { .. } => "needs_login",
            DelegateState::RateLimited { .. } => "rate_limited",
        }
    }

    fn version(&self) -> String {
        match self {
            DelegateState::NotInstalled => String::new(),
            DelegateState::Installed { version }
            | DelegateState::Ready { version }
            | DelegateState::NeedsLogin { version }
            | DelegateState::RateLimited { version } => version.clone(),
        }
    }
}

/// Build the version-probe invocation for `delegate`.
pub fn detect_spec(delegate: Delegate) -> CommandSpec {
    CommandSpec {
        program: delegate.binary().to_string(),
        args: delegate.version_args(),
        stdin: None,
        env_remove: SCRUBBED_ENV.iter().map(|s| s.to_string()).collect(),
        timeout: DETECT_TIMEOUT,
    }
}

/// Is `delegate` installed, and at what version? Runs `--version` only: no network, no quota, no
/// credential read.
pub fn detect(runner: &dyn CommandRunner, delegate: Delegate) -> DelegateState {
    match runner.run(&detect_spec(delegate)) {
        Ok(out) if out.ok() => DelegateState::Installed { version: parse_version(&out.stdout, &out.stderr) },
        // It exists but `--version` failed. Still installed — a broken version flag is not a reason
        // to hide a delegate the user can see in their own terminal.
        Ok(_) => DelegateState::Installed { version: String::new() },
        Err(RunError::NotFound) => DelegateState::NotInstalled,
        Err(_) => DelegateState::NotInstalled,
    }
}

/// Detect every delegate, in [`Delegate::ALL`] order.
pub fn detect_all(runner: &dyn CommandRunner) -> BTreeMap<Delegate, DelegateState> {
    Delegate::ALL.into_iter().map(|d| (d, detect(runner, d))).collect()
}

/// The version string, trimmed to one short safe line.
///
/// Version output is vendor-controlled text that ends up in a log line, so it is bounded and
/// scrubbed of anything credential-shaped rather than trusted.
fn parse_version(stdout: &str, stderr: &str) -> String {
    let raw = if stdout.trim().is_empty() { stderr } else { stdout };
    let line = raw.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let line = redact_secrets(line);
    match line.char_indices().nth(64) {
        Some((i, _)) => line[..i].to_string(),
        None => line,
    }
}

/// Run a real, minimal completion to learn whether the delegate is signed in.
///
/// This spends a token or two of the user's quota, so it is a **user-initiated** check ("Test
/// connection"), never something fired on every launch. It is also the only honest test available:
/// the alternative — inspecting the delegate's credential file — is the exact thing this module
/// refuses to do.
pub fn verify(runner: &dyn CommandRunner, delegate: Delegate) -> DelegateState {
    let state = detect(runner, delegate);
    if !state.is_installed() {
        return state;
    }
    let version = state.version();
    match runner.run(&completion_spec(delegate, "Reply with the single word: ok")) {
        Ok(out) if out.ok() => DelegateState::Ready { version },
        Ok(out) => match classify(&out) {
            DelegateFailure::NeedsLogin => DelegateState::NeedsLogin { version },
            DelegateFailure::RateLimited => DelegateState::RateLimited { version },
            // It ran, it was not an auth or quota refusal — the delegate itself is reachable, but we
            // have no proof of sign-in. Do not claim Ready.
            _ => DelegateState::Installed { version },
        },
        Err(RunError::NotFound) => DelegateState::NotInstalled,
        Err(_) => DelegateState::Installed { version },
    }
}

// ---------------------------------------------------------------- failure classification

/// Why a delegated run did not produce an answer. The distinctions exist because the user's next
/// step differs completely: install something, sign in somewhere, wait, or report a bug.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DelegateFailure {
    NotInstalled,
    NeedsLogin,
    /// The subscription's quota window is exhausted. Never SHOGUN's fault, and must not be reported
    /// as SHOGUN failing.
    RateLimited,
    Timeout,
    /// Anything else. Already passed through [`redact_secrets`] and truncated.
    Failed(String),
}

/// Substrings that mean "the delegate is not signed in".
const LOGIN_MARKERS: &[&str] = &[
    "not logged in",
    "not authenticated",
    "please log in",
    "please login",
    "run /login",
    "login required",
    "authentication required",
    "unauthorized",
    "invalid api key",
    "no credentials",
    "credentials not found",
    "sign in to",
    "http 401",
    "status 401",
];

/// Substrings that mean "the plan's quota window is exhausted". Checked before the login markers:
/// limit messages often also invite the user to sign in somewhere to upgrade, and calling a quota
/// problem an auth problem sends them to re-authenticate an account that was never broken.
const RATE_LIMIT_MARKERS: &[&str] = &[
    "rate limit",
    "rate_limit",
    "usage limit",
    "quota",
    "too many requests",
    "limit reached",
    "limit will reset",
    "http 429",
    "status 429",
];

/// Classify a finished-but-unsuccessful run from what the delegate printed.
pub fn classify(out: &CommandOutput) -> DelegateFailure {
    let hay = format!("{}\n{}", out.stderr, out.stdout).to_lowercase();
    if RATE_LIMIT_MARKERS.iter().any(|m| hay.contains(m)) {
        return DelegateFailure::RateLimited;
    }
    if LOGIN_MARKERS.iter().any(|m| hay.contains(m)) {
        return DelegateFailure::NeedsLogin;
    }
    DelegateFailure::Failed(short_reason(&out.stderr, &out.stdout))
}

/// The delegate's own explanation, redacted and bounded, for a one-line UI pill.
fn short_reason(stderr: &str, stdout: &str) -> String {
    let raw = if stderr.trim().is_empty() { stdout } else { stderr };
    let line = raw.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    let line = redact_secrets(line);
    match line.char_indices().nth(180) {
        Some((i, _)) => format!("{}…", &line[..i]),
        None => line,
    }
}

impl DelegateFailure {
    /// Turn a delegate failure into the lane's error type, keeping the distinctions the UI needs.
    pub fn into_llm_error(self, delegate: Delegate) -> LlmError {
        match self {
            DelegateFailure::NotInstalled => LlmError::NotConfigured,
            // Reuse Unauthorized so every existing 401 path — the sticky "credential rejected"
            // indicator, the settings prompt — lights up unchanged for a delegate that needs a login.
            DelegateFailure::NeedsLogin => LlmError::Unauthorized(
                401,
                format!("{} is not signed in — run `{}` once in a terminal to log in", delegate.label(), delegate.binary()),
            ),
            DelegateFailure::RateLimited => LlmError::RateLimited(format!(
                "{} usage limit reached on your {} plan — it resets on the vendor's schedule",
                delegate.label(),
                delegate.plan_label()
            )),
            DelegateFailure::Timeout => {
                LlmError::Provider(format!("{} did not answer in time", delegate.label()))
            }
            DelegateFailure::Failed(reason) if reason.is_empty() => {
                LlmError::Provider(format!("{} failed", delegate.label()))
            }
            DelegateFailure::Failed(reason) => {
                LlmError::Provider(format!("{}: {reason}", delegate.label()))
            }
        }
    }
}

// ---------------------------------------------------------------- agent client

/// Build the completion invocation for `delegate`, with `prompt` on stdin.
pub fn completion_spec(delegate: Delegate, prompt: &str) -> CommandSpec {
    CommandSpec {
        program: delegate.binary().to_string(),
        args: delegate.completion_args(),
        stdin: Some(prompt.to_string()),
        env_remove: SCRUBBED_ENV.iter().map(|s| s.to_string()).collect(),
        timeout: COMPLETION_TIMEOUT,
    }
}

/// The Agent-lane client that runs on the user's subscription via a local vendor CLI.
///
/// Implements [`AgentClient`] and **only** [`AgentClient`]. There is no [`BatchClient`](super::BatchClient)
/// implementation anywhere in this file, so indexing / Dream Cycle / Morning Brief cannot be routed
/// here even by a caller who wants to: invariant 5 stays a compile error, not a code review note.
pub struct SubscriptionAgentClient<R: CommandRunner, S: TraceabilitySink> {
    runner: R,
    sink: S,
    delegate: Delegate,
    purpose: &'static str,
}

impl<R: CommandRunner, S: TraceabilitySink> SubscriptionAgentClient<R, S> {
    pub fn new(runner: R, sink: S, delegate: Delegate) -> Self {
        Self { runner, sink, delegate, purpose: "agent" }
    }

    /// Override the traceability `purpose` (e.g. `"chat"` vs the default `"agent"`).
    pub fn with_purpose(mut self, purpose: &'static str) -> Self {
        self.purpose = purpose;
        self
    }

    pub fn delegate(&self) -> Delegate {
        self.delegate
    }
}

impl<R: CommandRunner, S: TraceabilitySink> AgentClient for SubscriptionAgentClient<R, S> {
    fn complete(&self, prompt: &str) -> Result<String, LlmError> {
        // Recorded before the run, not after: the trace must exist for a send that then fails
        // mid-flight, because the content left the process either way (AR-11).
        self.sink.record(TraceRecord::for_chunk(
            Route::LocalAgent,
            self.purpose,
            self.delegate.destination(),
            prompt,
            // The user's own vendor on the user's own plan. Not a relay like Composio, so no
            // third-party badge — the badge means "someone you did not choose saw this".
            false,
        ));

        let out = match self.runner.run(&completion_spec(self.delegate, prompt)) {
            Ok(out) => out,
            Err(RunError::NotFound) => {
                return Err(DelegateFailure::NotInstalled.into_llm_error(self.delegate))
            }
            Err(RunError::Timeout) => {
                return Err(DelegateFailure::Timeout.into_llm_error(self.delegate))
            }
            Err(RunError::Io(e)) => {
                return Err(DelegateFailure::Failed(e).into_llm_error(self.delegate))
            }
        };

        if !out.ok() {
            return Err(classify(&out).into_llm_error(self.delegate));
        }

        let text = out.stdout.trim();
        if text.is_empty() {
            // A zero exit with nothing on stdout is not an answer. Returning "" here would insert
            // an empty draft at the user's caret and report success.
            return Err(DelegateFailure::Failed("returned an empty response".into())
                .into_llm_error(self.delegate));
        }
        Ok(text.to_string())
    }
}

// ---------------------------------------------------------------- real runner

/// Runs delegates as real subprocesses.
///
/// Not compiled into the Linux test path's assertions — every rule in this module is tested through
/// [`MockRunner`] — but it is ordinary `std::process` code and builds everywhere.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError> {
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};

        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for name in &spec.env_remove {
            cmd.env_remove(name);
        }

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(RunError::NotFound),
            Err(e) => return Err(RunError::Io(redact_secrets(&e.to_string()))),
        };

        // Write the prompt and close stdin, on a thread: a delegate that starts emitting output
        // before it has drained a large prompt would deadlock a same-thread write.
        let writer = spec.stdin.clone().and_then(|body| {
            child.stdin.take().map(|mut pipe| {
                std::thread::spawn(move || {
                    let _ = pipe.write_all(body.as_bytes());
                    // Dropping `pipe` closes stdin, which is how the delegate knows the prompt ended.
                })
            })
        });
        if spec.stdin.is_none() {
            drop(child.stdin.take());
        }

        // Drain both pipes concurrently. A single-pipe read would block forever the moment the
        // other filled its buffer.
        let out_reader = child.stdout.take().map(|mut p| {
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = p.read_to_string(&mut s);
                s
            })
        });
        let err_reader = child.stderr.take().map(|mut p| {
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = p.read_to_string(&mut s);
                s
            })
        });

        let deadline = std::time::Instant::now() + spec.timeout;
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {
                    if std::time::Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        timed_out = true;
                        break None;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(RunError::Io(redact_secrets(&e.to_string()))),
            }
        };

        if let Some(h) = writer {
            let _ = h.join();
        }
        let stdout = out_reader.and_then(|h| h.join().ok()).unwrap_or_default();
        let stderr = err_reader.and_then(|h| h.join().ok()).unwrap_or_default();

        if timed_out {
            return Err(RunError::Timeout);
        }
        Ok(CommandOutput { code: status.and_then(|s| s.code()), stdout, stderr })
    }
}

// ---------------------------------------------------------------- mock runner

/// A scripted runner for tests: records every [`CommandSpec`] it is handed and replies from a queue
/// of canned outcomes. Public so the desktop crate can test its wiring without a vendor CLI.
pub struct MockRunner {
    seen: std::sync::Mutex<Vec<CommandSpec>>,
    replies: std::sync::Mutex<std::collections::VecDeque<Result<CommandOutput, RunError>>>,
    fallback: Result<CommandOutput, RunError>,
}

impl MockRunner {
    /// A runner that replies with `replies` in order, then repeats `fallback` forever.
    pub fn new(
        replies: Vec<Result<CommandOutput, RunError>>,
        fallback: Result<CommandOutput, RunError>,
    ) -> Self {
        Self {
            seen: std::sync::Mutex::new(Vec::new()),
            replies: std::sync::Mutex::new(replies.into()),
            fallback,
        }
    }

    /// A runner that always answers `out`.
    pub fn always(out: Result<CommandOutput, RunError>) -> Self {
        Self::new(Vec::new(), out)
    }

    /// Every spec this runner was asked to execute, in order.
    pub fn seen(&self) -> Vec<CommandSpec> {
        self.seen.lock().map(|g| g.clone()).unwrap_or_default()
    }
}

impl CommandRunner for MockRunner {
    fn run(&self, spec: &CommandSpec) -> Result<CommandOutput, RunError> {
        if let Ok(mut g) = self.seen.lock() {
            g.push(spec.clone());
        }
        self.replies
            .lock()
            .ok()
            .and_then(|mut g| g.pop_front())
            .unwrap_or_else(|| self.fallback.clone())
    }
}

/// Shorthand for a successful run.
pub fn out_ok(stdout: &str) -> Result<CommandOutput, RunError> {
    Ok(CommandOutput { code: Some(0), stdout: stdout.to_string(), stderr: String::new() })
}

/// Shorthand for a failed run carrying `stderr`.
pub fn out_err(code: i32, stderr: &str) -> Result<CommandOutput, RunError> {
    Ok(CommandOutput { code: Some(code), stdout: String::new(), stderr: stderr.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::traceability::RecordingSink;

    // ---- non-goals: the guarantees that make this approach legitimate --------------------

    #[test]
    fn no_invocation_ever_touches_a_credential_store() {
        // The whole design rests on SHOGUN never lifting a vendor's stored token. Every command
        // this module can construct is checked against the paths a credential lift would need.
        const FORBIDDEN: &[&str] = &[
            ".credentials",
            "auth.json",
            "keychain",
            "security find-generic-password",
            "oauth",
            "access_token",
            "refresh_token",
        ];
        let mut specs = Vec::new();
        for d in Delegate::ALL {
            specs.push(detect_spec(d));
            specs.push(completion_spec(d, "hello"));
        }
        for spec in specs {
            let line = format!("{} {}", spec.program, spec.args.join(" ")).to_lowercase();
            for bad in FORBIDDEN {
                assert!(!line.contains(bad), "invocation reaches for a credential: {line}");
            }
        }
    }

    #[test]
    fn the_prompt_travels_on_stdin_never_in_argv() {
        // argv is readable by every process on the machine via `ps`. Captured user text in there
        // would be a privacy breach that no amount of care at the call sites could fix.
        let secret = "quarterly numbers are down 40%";
        for d in Delegate::ALL {
            let spec = completion_spec(d, secret);
            assert_eq!(spec.stdin.as_deref(), Some(secret), "{} must take the prompt on stdin", d.id());
            for arg in &spec.args {
                assert!(!arg.contains("quarterly"), "{} leaked the prompt into argv", d.id());
            }
        }
    }

    #[test]
    fn our_own_api_keys_are_scrubbed_from_every_child() {
        // Left in place, a delegate would authenticate with SHOGUN's metered key and quietly bill
        // the user per token — the exact outcome they chose a subscription to avoid.
        for d in Delegate::ALL {
            for spec in [detect_spec(d), completion_spec(d, "x")] {
                assert!(spec.env_remove.iter().any(|e| e == "ANTHROPIC_API_KEY"));
                assert!(spec.env_remove.iter().any(|e| e == "OPENAI_API_KEY"));
                assert!(spec.env_remove.iter().any(|e| e == "CLAUDE_CODE_OAUTH_TOKEN"));
            }
        }
    }

    // ---- ids ------------------------------------------------------------------------------

    #[test]
    fn delegate_ids_roundtrip_and_are_distinct() {
        let mut ids: Vec<&str> = Delegate::ALL.iter().map(|d| d.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Delegate::ALL.len(), "delegate ids must be unique");
        for d in Delegate::ALL {
            assert_eq!(Delegate::parse(d.id()), Some(d));
        }
        assert_eq!(Delegate::parse("gpt-5-cli"), None);
    }

    #[test]
    fn destination_names_the_vendor_and_the_local_hop() {
        // The traceability screen answers "who saw this". Naming only the binary would hide the
        // vendor; naming only the vendor would hide that it left through another process.
        let d = Delegate::ClaudeCode.destination();
        assert!(d.contains("api.anthropic.com"), "{d}");
        assert!(d.contains("claude"), "{d}");
    }

    // ---- detection -------------------------------------------------------------------------

    #[test]
    fn a_missing_binary_is_not_installed() {
        let runner = MockRunner::always(Err(RunError::NotFound));
        assert_eq!(detect(&runner, Delegate::Codex), DelegateState::NotInstalled);
    }

    #[test]
    fn detection_reads_the_version_but_never_claims_ready() {
        // Presence is not sign-in. Promoting "installed" to "ready" would end onboarding on a
        // success screen and fail on the user's first real action.
        let runner = MockRunner::always(out_ok("1.2.3 (Claude Code)\n"));
        let state = detect(&runner, Delegate::ClaudeCode);
        assert_eq!(state, DelegateState::Installed { version: "1.2.3 (Claude Code)".into() });
        assert!(!state.is_usable());
        assert!(state.is_installed());
    }

    #[test]
    fn detect_all_covers_every_delegate() {
        let runner = MockRunner::always(Err(RunError::NotFound));
        let all = detect_all(&runner);
        assert_eq!(all.len(), Delegate::ALL.len());
        assert!(all.values().all(|s| !s.is_installed()));
    }

    #[test]
    fn a_credential_shaped_version_string_is_redacted() {
        // Version output is vendor text that lands in a log line; it must not carry anything
        // credential-shaped through (invariant 7).
        let fake = format!("sk-{}", "a".repeat(40));
        let runner = MockRunner::always(out_ok(&format!("1.0.0 {fake}")));
        let state = detect(&runner, Delegate::ClaudeCode);
        assert!(!format!("{state:?}").contains(&fake), "{state:?}");
    }

    #[test]
    fn verify_promotes_to_ready_only_after_a_real_completion() {
        let runner = MockRunner::new(vec![out_ok("2.0.0"), out_ok("ok")], out_ok(""));
        assert_eq!(verify(&runner, Delegate::ClaudeCode), DelegateState::Ready { version: "2.0.0".into() });
    }

    #[test]
    fn verify_reports_needs_login_without_reading_credentials() {
        let runner = MockRunner::new(
            vec![out_ok("2.0.0"), out_err(1, "Error: not logged in. Run `claude` to sign in.")],
            out_ok(""),
        );
        assert_eq!(verify(&runner, Delegate::ClaudeCode), DelegateState::NeedsLogin { version: "2.0.0".into() });
    }

    #[test]
    fn verify_reports_rate_limited_separately_from_needs_login() {
        let runner = MockRunner::new(
            vec![out_ok("2.0.0"), out_err(1, "usage limit reached; resets at 5pm")],
            out_ok(""),
        );
        assert_eq!(verify(&runner, Delegate::ClaudeCode), DelegateState::RateLimited { version: "2.0.0".into() });
    }

    #[test]
    fn verify_of_a_missing_binary_stays_not_installed() {
        let runner = MockRunner::always(Err(RunError::NotFound));
        assert_eq!(verify(&runner, Delegate::GeminiCli), DelegateState::NotInstalled);
    }

    // ---- classification --------------------------------------------------------------------

    #[test]
    fn quota_messages_beat_login_messages() {
        // Vendors word limit errors as "usage limit reached — sign in to upgrade". Reading that as
        // an auth failure sends the user to re-authenticate an account that works fine.
        let out = CommandOutput {
            code: Some(1),
            stdout: String::new(),
            stderr: "Usage limit reached. Sign in to your account to upgrade.".into(),
        };
        assert_eq!(classify(&out), DelegateFailure::RateLimited);
    }

    #[test]
    fn a_401_is_a_login_problem() {
        let out = CommandOutput { code: Some(1), stdout: String::new(), stderr: "request failed: HTTP 401".into() };
        assert_eq!(classify(&out), DelegateFailure::NeedsLogin);
    }

    #[test]
    fn an_unrecognised_failure_keeps_the_delegates_own_words() {
        let out = out_err(2, "model overloaded, try again").unwrap();
        match classify(&out) {
            DelegateFailure::Failed(r) => assert!(r.contains("overloaded"), "{r}"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn a_failure_quoting_a_credential_is_redacted() {
        let fake = format!("sk-ant-{}", "z".repeat(40));
        let out = out_err(1, &format!("bad request with {fake}")).unwrap();
        let shown = format!("{:?}", classify(&out));
        assert!(!shown.contains(&fake), "credential survived classification: {shown}");
    }

    #[test]
    fn rate_limit_error_names_the_plan_and_does_not_blame_shogun() {
        let e = DelegateFailure::RateLimited.into_llm_error(Delegate::ClaudeCode);
        assert!(matches!(e, LlmError::RateLimited(_)));
        let msg = e.to_string();
        assert!(msg.contains("Claude Pro / Max"), "{msg}");
        assert!(!msg.to_lowercase().contains("shogun"), "{msg}");
    }

    #[test]
    fn needs_login_maps_onto_the_existing_401_path() {
        // So the sticky "credential rejected" indicator and the settings prompt light up unchanged.
        let e = DelegateFailure::NeedsLogin.into_llm_error(Delegate::Codex);
        assert!(matches!(e, LlmError::Unauthorized(401, _)), "{e:?}");
    }

    // ---- the client ------------------------------------------------------------------------

    #[test]
    fn a_successful_completion_returns_the_trimmed_answer() {
        let client = SubscriptionAgentClient::new(
            MockRunner::always(out_ok("  Sounds good — shipping Friday.\n")),
            RecordingSink::new(),
            Delegate::ClaudeCode,
        );
        assert_eq!(client.complete("draft a reply").unwrap(), "Sounds good — shipping Friday.");
    }

    #[test]
    fn every_completion_writes_exactly_one_trace_with_no_text() {
        let prompt = "reply about the acquisition of Northwind";
        let client = SubscriptionAgentClient::new(
            MockRunner::always(out_ok("done")),
            RecordingSink::new(),
            Delegate::ClaudeCode,
        );
        client.complete(prompt).unwrap();
        let recs = client.sink.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].route, Route::LocalAgent);
        assert_eq!(recs[0].chunk_bytes, prompt.len());
        assert!(!recs[0].third_party, "the user's own plan is not a third-party relay");
        assert!(!format!("{:?}", recs[0]).contains("Northwind"), "prompt text reached the trace");
    }

    #[test]
    fn a_failed_run_is_still_traced() {
        // The content left this process before the delegate refused. AR-11 records egress, not
        // success.
        let client = SubscriptionAgentClient::new(
            MockRunner::always(out_err(1, "not logged in")),
            RecordingSink::new(),
            Delegate::ClaudeCode,
        );
        assert!(client.complete("hello").is_err());
        assert_eq!(client.sink.records().len(), 1);
    }

    #[test]
    fn an_empty_answer_is_an_error_not_an_empty_draft() {
        // Exit 0 with nothing on stdout would otherwise insert an empty draft at the user's caret
        // and report success.
        let client = SubscriptionAgentClient::new(
            MockRunner::always(out_ok("   \n")),
            RecordingSink::new(),
            Delegate::ClaudeCode,
        );
        assert!(client.complete("hi").is_err());
    }

    #[test]
    fn a_missing_delegate_reports_not_configured() {
        let client = SubscriptionAgentClient::new(
            MockRunner::always(Err(RunError::NotFound)),
            RecordingSink::new(),
            Delegate::Codex,
        );
        assert!(matches!(client.complete("hi"), Err(LlmError::NotConfigured)));
    }

    #[test]
    fn a_timeout_is_reported_as_such() {
        let client = SubscriptionAgentClient::new(
            MockRunner::always(Err(RunError::Timeout)),
            RecordingSink::new(),
            Delegate::GeminiCli,
        );
        let msg = client.complete("hi").unwrap_err().to_string();
        assert!(msg.contains("in time"), "{msg}");
    }

    #[test]
    fn the_client_runs_the_delegate_it_was_built_with() {
        let client = SubscriptionAgentClient::new(
            MockRunner::always(out_ok("x")),
            RecordingSink::new(),
            Delegate::Codex,
        );
        client.complete("hi").unwrap();
        let seen = client.runner.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].program, "codex");
    }

    // Invariant 5, stated where it is enforced: `SubscriptionAgentClient` implements `AgentClient`
    // and nothing else. There is no `impl BatchClient for SubscriptionAgentClient`, so the
    // following does not compile, and indexing / Dream Cycle / Morning Brief cannot be pushed
    // through a subscription quota:
    //     fn f(c: &dyn super::super::BatchClient) {}
    //     f(&SubscriptionAgentClient::new(..., Delegate::ClaudeCode));
}
