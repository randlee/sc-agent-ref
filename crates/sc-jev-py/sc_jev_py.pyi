"""Type stubs for the sc_jev_py extension module (PyO3 over sc-jev-check)."""

import os
import pathlib
from typing import Any, Iterable, Iterator, Mapping, Sequence

StrPath = str | os.PathLike[str]

class JevError(Exception):
    """Jev check failed.

    The attributes are the keys of the `sc-ai` stderr error body, from the same
    serialization (`code` is the body's `error`). `str(exc)` is
    `CODE: message. suggested_action`.
    """

    code: str
    """The stable error id, such as `PROVIDER_BUSY` or `IO_FAILED`."""
    kind: str
    """The broad class: `validation`, `not_found`, `invalid_state`,
    `dependency`, `permission`, `timeout` or `capacity`."""
    message: str
    """What failed and why. Never the fix."""
    suggested_action: str
    """What to do next."""
    retryable: bool
    """Whether retrying later may succeed. Always a bool."""
    retry_after_seconds: int | None
    """The provider's `Retry-After` in whole seconds, when known."""
    field: str | None
    """The failing flag, argument or provider-named request field."""
    details: dict[str, Any] | None
    """Machine fields by id (`http_status`, `cause`, `attempts`, `limit`,
    `actual`, `unit`, `what`, `io_kind`, `network_kind`, `provider_type`,
    `log_flush_failure`, ...); see docs/stability.md."""
    schema_version: int
    """The error shape version; always 1."""
    response: dict[str, Any] | None
    """Set when any backend failed: the run response body (`data`). It holds
    the answers that succeeded, if any, and every failed backend's typed error
    with `details.backend`. `None` otherwise."""

class Session:
    """Live judge session returned by `CheckResult.session`."""

    @property
    def session_id(self) -> str:
        """The session id: a 26-character ULID."""
    def ask(self, follow_up: str, *, project: str | None = None) -> dict[str, Any]:
        """Send a follow-up. `project` labels this call's log row only."""

class CheckResult:
    """Result of `run_check`. Immutable."""

    @property
    def session_id(self) -> str | None: ...
    @property
    def prompt_sha256(self) -> str: ...
    @property
    def payload(self) -> str: ...
    @property
    def outcomes(self) -> list[dict[str, Any]]: ...
    @property
    def equal_input(self) -> dict[str, Any] | None: ...
    @property
    def data(self) -> dict[str, Any]: ...
    @property
    def system_prompt(self) -> str | None: ...
    @property
    def session(self) -> Session | None: ...
    def ask(self, follow_up: str, *, project: str | None = None) -> dict[str, Any]: ...

def build_request(
    check_path: StrPath,
    *,
    repo_root: StrPath | None = None,
    rev: str = "WORKTREE",
) -> dict[str, Any]: ...
def run_check(
    check_path: StrPath,
    *,
    backends: Sequence[str] | None = None,
    dry_run: bool = False,
    repo_root: StrPath | None = None,
    rev: str = "WORKTREE",
    log_dir: StrPath | None = None,
    session_dir: StrPath | None = None,
    system_prompt_file: StrPath | None = None,
    chain_context: bool = False,
    project: str | None = None,
) -> CheckResult:
    """Run a check. `project` labels this run's log rows (at most 256 bytes,
    no control characters; stored as given, not scrubbed)."""
class DecisionStream:
    """Answers many requests in one process: a bounded queue and N workers.

    `submit(id, request)` queues a request (a dict or JSON text) and `next()`
    (or iteration) returns answers as they complete, in completion order, each
    with the `id` it was submitted with. An answer is a dict
    `{"id", "ok", "envelope"?, "error"?}`; a failed request is `ok == False`
    with an `error` dict, never an exception, and the stream continues. The
    stream is finished when `close_input()` was called, the queue is empty and
    the workers are idle. The GIL is released while workers run and while
    waiting. `concurrency` is 1 to 64; the default 8 was not tested against
    live providers.
    """

    def __init__(
        self,
        *,
        concurrency: int = 8,
        backends: Sequence[str] | None = None,
        repo_root: StrPath | None = None,
        log_dir: StrPath | None = None,
        session_dir: StrPath | None = None,
        system_prompt_file: StrPath | None = None,
        chain_context: bool = False,
        project: str | None = None,
    ) -> None: ...
    def submit(
        self,
        id: str,
        request: Mapping[str, Any] | str,
        *,
        timeout: float | None = None,
    ) -> bool:
        """Queue a request; blocks while the queue is full. False on timeout.

        An unread answer holds a place: past `capacity` unread answers a single
        thread blocks forever unless it reads from another thread or passes
        `timeout=`.
        """
    def submit_line(self, line: str, *, timeout: float | None = None) -> bool:
        """Queue one `{"id", "request"}` JSONL line, as `sc-ai jev stream` reads it."""
    def next(self, timeout: float | None = None) -> dict[str, Any] | None:
        """The next answer, or None once finished. TimeoutError on timeout."""
    def __iter__(self) -> Iterator[dict[str, Any]]: ...
    def __next__(self) -> dict[str, Any]: ...
    def close_input(self) -> None: ...
    def cancel(self) -> None: ...
    @property
    def in_flight(self) -> int: ...
    @property
    def capacity(self) -> int: ...
    def finish(self) -> dict[str, Any]:
        """Close input, drop unread answers, wait for workers, flush the log.

        Returns `{"answered", "failed", "exit_code", "log_flush_warning"}`.
        """
    def __enter__(self) -> DecisionStream: ...
    def __exit__(self, *exc: object) -> None: ...

def run_many(
    requests: Mapping[str, Mapping[str, Any] | str] | Iterable[tuple[str, Mapping[str, Any] | str]],
    *,
    concurrency: int = 8,
    backends: Sequence[str] | None = None,
    repo_root: StrPath | None = None,
    log_dir: StrPath | None = None,
    session_dir: StrPath | None = None,
    system_prompt_file: StrPath | None = None,
    chain_context: bool = False,
    project: str | None = None,
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Run a batch in parallel: `(answers, summary)`.

    Answers are in completion order with ids echoed; the summary is
    `finish()`'s. Every id and body is validated before anything starts.
    """
def canonical_bytes(json_text: str) -> bytes: ...
def canonical_sha256(json_text: str) -> str: ...
def default_log_root() -> pathlib.Path: ...
def resolve_log_root(log_dir: StrPath | None = None) -> pathlib.Path: ...
def resolve_session_dir(
    log_dir: StrPath, *, session_dir: StrPath | None = None
) -> pathlib.Path: ...
def log_file(log_dir: StrPath) -> pathlib.Path: ...

