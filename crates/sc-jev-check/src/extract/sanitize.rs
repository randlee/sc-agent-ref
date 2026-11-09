//! Child-process environment and stderr hygiene.
//!
//! Extractor and git children never inherit provider credentials. Their
//! stderr reaches [`JevError`] details only as a bounded tail with two kinds
//! of text replaced by `[redacted]`: the value of any credential-looking
//! environment variable of this process (`*_API_KEY`, `SC_*`), and any
//! bearer-token shape, whatever its source (`Bearer <token>`, or an
//! `sk-` prefixed key). A secret of another shape that is not in this
//! process's environment is not recognized.

use std::ffi::{OsStr, OsString};
use std::process::Command;

use sc_agent_types::JevError;

/// Bytes of child stderr kept for an error detail.
pub(crate) const STDERR_TAIL_BYTES: usize = 4096;

/// Shortest environment value treated as a secret when redacting.
///
/// Shorter values (`1`, `true`) would redact ordinary words.
const MIN_SECRET_LEN: usize = 8;

/// Variables an extractor child may inherit (`LC_*` is matched by prefix).
const ALLOWED_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USERPROFILE",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "LANG",
    "TERM",
];

/// Clears `command`'s environment and copies only the allowlist from this process.
///
/// No `*_API_KEY` or `SC_*` variable can pass: neither matches an allowlist
/// entry or the `LC_` prefix.
pub(crate) fn apply_env_allowlist(command: &mut Command) {
    command.env_clear();
    for (name, value) in std::env::vars_os() {
        if is_allowed(&name) {
            command.env(name, value);
        }
    }
}

fn is_allowed(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let upper = name.to_ascii_uppercase();
    upper.starts_with("LC_") || ALLOWED_ENV.contains(&upper.as_str())
}

fn is_secret_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.ends_with("_API_KEY") || upper.starts_with("SC_")
}

/// Values of credential-looking variables in this process.
fn secret_values() -> Vec<OsString> {
    std::env::vars_os()
        .filter(|(name, value)| {
            name.to_str().is_some_and(is_secret_name) && value.len() >= MIN_SECRET_LEN
        })
        .map(|(_, value)| value)
        .collect()
}

/// Replaces every credential-looking environment value in `text`.
fn redact_text(text: &str, secrets: &[OsString]) -> String {
    let mut out = text.to_owned();
    for secret in secrets {
        if let Some(secret) = secret.to_str() {
            out = out.replace(secret, REDACTED);
        }
    }
    out
}

const REDACTED: &str = "[redacted]";

/// Fewest characters after `sk-` that make a token key-shaped.
const MIN_KEY_TAIL: usize = 8;

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~' | b'+' | b'/' | b'=')
}

/// End of the run of token bytes starting at `start`.
fn token_end(bytes: &[u8], start: usize) -> usize {
    bytes[start..]
        .iter()
        .position(|byte| !is_token_byte(*byte))
        .map_or(bytes.len(), |length| start + length)
}

/// Replaces bearer-token shapes in `text`, whether or not the token is held
/// in this process's environment:
///
/// - `Bearer <token>` (any case, spaces or tabs between): the token goes,
///   the word `Bearer` stays so the line still reads;
/// - a key-shaped token `sk-` plus at least [`MIN_KEY_TAIL`] token
///   characters, at a word start: the whole token goes.
///
/// A token is a run of letters, digits and `-._~+/=`.
fn redact_shapes(text: &str) -> String {
    const BEARER: &[u8] = b"bearer";
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        let word_start = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        // Cut points are ASCII bytes, so slicing `text` there is valid.
        let cut = if !word_start {
            None
        } else if bytes[at..]
            .get(..BEARER.len())
            .is_some_and(|word| word.eq_ignore_ascii_case(BEARER))
        {
            let after = at + BEARER.len();
            let gap = bytes[after..]
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            let start = after + gap;
            (gap > 0 && token_end(bytes, start) > start).then(|| (start, token_end(bytes, start)))
        } else if bytes[at..].starts_with(b"sk-") {
            let end = token_end(bytes, at);
            (end - at >= b"sk-".len() + MIN_KEY_TAIL).then_some((at, end))
        } else {
            None
        };
        if let Some((start, end)) = cut {
            out.push_str(&text[copied..start]);
            out.push_str(REDACTED);
            copied = end;
            at = end;
        } else {
            at += 1;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// Redacts credential-looking environment values and bearer-token shapes
/// from every text field of `error`.
pub(crate) fn redact_error(error: JevError) -> JevError {
    let secrets = secret_values();
    error.map_detail(|text| redact_shapes(&redact_text(text, &secrets)))
}

/// Last [`STDERR_TAIL_BYTES`] of `stderr` as trimmed, lossy UTF-8.
///
/// Redaction happens when the text is placed in an error, through
/// [`redact_error`].
pub(crate) fn stderr_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(STDERR_TAIL_BYTES);
    String::from_utf8_lossy(&stderr[start..]).trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_admits_locale_and_path_only() {
        for name in ["PATH", "Path", "LC_ALL", "LC_CTYPE", "LANG", "TMPDIR"] {
            assert!(is_allowed(OsStr::new(name)), "{name}");
        }
        for name in [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "TYPESAFE_API_KEY",
            "SC_LOG_ROOT",
            "SC_ANY_SECRET",
            "GIT_DIR",
        ] {
            assert!(!is_allowed(OsStr::new(name)), "{name}");
        }
    }

    #[test]
    fn redact_text_replaces_secret_values() {
        let secrets = vec![OsString::from("sk-secret-value-123")];
        assert_eq!(
            redact_text("failed with sk-secret-value-123 here", &secrets),
            "failed with [redacted] here"
        );
    }

    #[test]
    fn redact_shapes_covers_bearer_tokens_and_sk_keys() {
        for (input, expected) in [
            (
                "401: Authorization: Bearer sk-live-0123456789abcdef",
                "401: Authorization: Bearer [redacted]",
            ),
            (
                "bearer\tabc.def-ghi_jkl==\nnext",
                "bearer\t[redacted]\nnext",
            ),
            (
                "BEARER   tok123 and BeArEr  x.y",
                "BEARER   [redacted] and BeArEr  [redacted]",
            ),
            (
                "key sk-ant-api03-abcdefghij failed",
                "key [redacted] failed",
            ),
            ("(sk-proj-ABCDEFGH12345678)", "([redacted])"),
            (
                "non-ascii \u{e9} Bearer abc\u{e9}",
                "non-ascii \u{e9} Bearer [redacted]\u{e9}",
            ),
        ] {
            assert_eq!(redact_shapes(input), expected, "{input}");
        }
        // Plain text, a short `sk-` word and a word that ends in `sk-` stay.
        for input in [
            "no secrets here",
            "sk-short",
            "task-assessment-complete",
            "risk-assessment-completed",
            "Bearer",
            "Bearer ",
            "bearers are people",
        ] {
            assert_eq!(redact_shapes(input), input, "{input}");
        }
    }

    #[test]
    fn redact_error_covers_a_literal_bearer_token_without_an_environment_secret() {
        let error = redact_error(JevError::extraction_failed(
            "curl: (22) 401\nAuthorization: Bearer sk-literal-not-in-env-0123456789",
        ));
        let rendered = format!("{error} {}", error.message());
        assert!(!rendered.contains("sk-literal"), "{rendered}");
        assert!(rendered.contains("Bearer [redacted]"), "{rendered}");
    }

    #[test]
    fn stderr_tail_keeps_the_end() {
        let mut stderr = vec![b'x'; STDERR_TAIL_BYTES * 2];
        stderr.extend_from_slice(b"final line\n");
        let tail = stderr_tail(&stderr);
        assert!(tail.ends_with("final line"));
        assert!(tail.len() <= STDERR_TAIL_BYTES);
    }
}
