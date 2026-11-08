//! Session-log row emission (REQ-SAG-0610, REQ-SAG-0617, ADR-SAG-0031).
//!
//! Every send that reaches a backend's `judge` ends in exactly one
//! [`LogEvent`] whose `fields{}` is one `sc_decision_schema::session_log`
//! row, `Ok` or `Failed`. A backend refused before send (`NotSent`) emits
//! none. The log is best effort: a rejected event never fails the run.

use std::time::SystemTime;

use sc_agent_types::observability::v2::{LogEvent, SharedSessionLog};
use sc_agent_types::{AnswerId, JevError, Operation, ProjectLabel, RepoRoot, SendTrace, SessionId};
use sc_decision_schema::session_log::{RowContext, RowContextBuilder, SessionLogRow, TimestampMs};
use sc_decision_schema::{DecisionSchemaId, JudgeBackend, JudgeOutcome, JudgeRequest};

use super::{RunConfig, discover_main_repo_root};

/// Who and where a send belongs to, for the log row's `repo` and `project`
/// columns.
///
/// `repo` changes only the log field. Session and log paths keep using the
/// worktree's own root (ADR-SAG-0021). `project` is caller text, stored as
/// given and not scrubbed (ADR-SAG-0031).
///
/// # Examples
///
/// ```
/// use sc_agent_types::{ProjectLabel, RepoRoot};
/// use sc_jev_check::run::LogContext;
///
/// let context = LogContext::new(
///     RepoRoot::new("/work/repos/app"),
///     Some(ProjectLabel::new("billing")?),
/// );
/// assert_eq!(context.project().map(ProjectLabel::as_str), Some("billing"));
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
#[derive(Debug, Clone)]
pub struct LogContext {
    repo: RepoRoot,
    project: Option<ProjectLabel>,
}

impl LogContext {
    /// A context for `repo` (the main repository root) and an optional
    /// `project`.
    #[must_use]
    pub fn new(repo: RepoRoot, project: Option<ProjectLabel>) -> Self {
        Self { repo, project }
    }

    /// The repository logged in the row's `repo` column.
    #[must_use]
    pub fn repo(&self) -> &RepoRoot {
        &self.repo
    }

    /// The caller's project label, when one was given.
    #[must_use]
    pub fn project(&self) -> Option<&ProjectLabel> {
        self.project.as_ref()
    }
}

/// What the send step needs to write one row: fixed per run or `ask`.
pub(crate) struct SendLog<'a> {
    pub(crate) log: &'a SharedSessionLog,
    pub(crate) session_id: SessionId,
    pub(crate) session_created_at: SystemTime,
    pub(crate) repo: String,
    pub(crate) project: Option<ProjectLabel>,
    pub(crate) operation: Operation,
}

impl<'a> SendLog<'a> {
    /// Resolves the log context of `config`: the one set with
    /// [`RunConfig::log_context`], else the main repository of the config's
    /// repo root and no project. Discovery failure falls back to the repo
    /// root as given, because logging is best effort.
    pub(crate) fn new(
        config: &'a RunConfig,
        session_id: SessionId,
        session_created_at: SystemTime,
        operation: Operation,
    ) -> Self {
        let (repo, project) = match &config.log_context {
            Some(context) => (context.repo.clone(), context.project.clone()),
            None => (
                discover_main_repo_root(config.repo_root.as_path())
                    .unwrap_or_else(|_| config.repo_root.clone()),
                None,
            ),
        };
        Self {
            log: &config.session_log,
            session_id,
            session_created_at,
            repo: RowContext::repo_text(repo.as_path()),
            project,
            operation,
        }
    }

    /// Writes the row for `judged`, the result of one `judge` call.
    fn emit(
        &self,
        judge: &dyn JudgeBackend,
        request: &JudgeRequest,
        trace: &SendTrace,
        judged: &Result<JudgeOutcome, JevError>,
    ) {
        let built = RowContextBuilder::new()
            .answer_id(AnswerId::new())
            .session_id(self.session_id)
            .session_created_at(TimestampMs::from_system_time(self.session_created_at))
            .repo(self.repo.clone())
            .project(self.project.clone())
            .payload_sha256(request.payload().sha256().clone())
            .decision_schema(DecisionSchemaId::CURRENT)
            .question_ids(request.payload().question_ids().cloned().collect())
            .request_sha256(trace.request_sha256())
            .ts(TimestampMs::from_system_time(SystemTime::now()))
            .operation(self.operation)
            .backend(judge.id())
            .model_id(judge.model_id())
            .build();
        // Every required value is set above, so `build` cannot fail; a row
        // that could not be built is skipped like any rejected row.
        let Ok(context) = built else {
            return;
        };
        let row = match judged {
            Ok(outcome) => SessionLogRow::ok(context, outcome.answers().clone()),
            Err(error) => SessionLogRow::failed(context, error.code()),
        };
        // Best effort: a rejected row never fails the run (REQ-SAG-0617).
        let _ = self.log.emit(LogEvent::new(
            self.session_id,
            self.operation,
            row.to_fields(),
        ));
    }
}

/// The engine's one send step: `judge` with a fresh [`SendTrace`], then one
/// log row (sent or failed), then the judge's result. Callers validate first.
pub(crate) fn send(
    judge: &dyn JudgeBackend,
    request: &JudgeRequest,
    log: &SendLog<'_>,
) -> Result<JudgeOutcome, JevError> {
    // One trace per backend send, one row per trace: never per run, never
    // shared through a cloned `JudgeRequest` (REQ-SAG-0613).
    let trace = SendTrace::new();
    let judged = judge.judge(request, &trace);
    log.emit(judge, request, &trace, &judged);
    judged
}
