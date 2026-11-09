"""Type stubs for the sc_jev_py extension module (PyO3 over sc-jev-check)."""

import os
import pathlib
from typing import Any, Callable, Sequence

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
    def payload_sha256(self) -> str: ...
    @property
    def questions_sha256(self) -> str: ...
    @property
    def request_sha256(self) -> dict[str, str]: ...
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
    check_path: StrPath | None = None,
    *,
    request_json: str | bytes | None = None,
    repo_root: StrPath | None = None,
    rev: str = "WORKTREE",
) -> dict[str, Any]: ...
def run_check(
    check_path: StrPath | None = None,
    *,
    request_json: str | bytes | None = None,
    backends: Sequence[str] | None = None,
    dry_run: bool = False,
    repo_root: StrPath | None = None,
    rev: str = "WORKTREE",
    log_dir: StrPath | None = None,
    session_dir: StrPath | None = None,
    system_prompt_file: StrPath | None = None,
    chain_context: bool = False,
    project: str | None = None,
    session_log: Callable[[dict[str, Any]], object] | None = None,
    general_log: Callable[[dict[str, Any]], object] | None = None,
) -> CheckResult:
    """Run a check. `project` labels this run's log rows (at most 256 bytes,
    no control characters; stored as given, not scrubbed).

    Two logs are written to separate files. `session_log` receives
    `{"session_id", "operation", "fields"}` for each judge send; `general_log`
    receives `{"level", "target", "action", "message", "fields"}` for each
    diagnostic event. Each replaces its sc-agent default; unset, the session
    log is `log_file(log_dir)` and the general log is
    `general_log_file(log_dir)`. A callable that raises is ignored: logging
    never changes a result. A callable needs the GIL once per row; the
    default general log does not."""
def canonical_bytes(json_text: str) -> bytes: ...
def canonical_sha256(json_text: str) -> str: ...
def default_log_root() -> pathlib.Path: ...
def resolve_log_root(log_dir: StrPath | None = None) -> pathlib.Path: ...
def resolve_session_dir(
    log_dir: StrPath, *, session_dir: StrPath | None = None
) -> pathlib.Path: ...
def log_file(log_dir: StrPath) -> pathlib.Path: ...
def general_log_file(log_dir: StrPath) -> pathlib.Path:
    """`{log_dir}/logs/sc-jev-py-general.log.jsonl`, the default general log."""
def install_host_logger(log_dir: StrPath | None = None) -> pathlib.Path:
    """Start the shared host logger and return the file it writes.

    Rust writes the default general log through this logger with no GIL, and
    `sc_observability.get_host_logger()` returns a logger over the same queue
    and file. Import `sc_jev_py` before `sc_observability`. One process has
    one host logger: a later call returns the first logger's file.
    """
