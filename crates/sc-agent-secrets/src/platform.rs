//! Process environment and platform keyring adapters.
//!
//! Keyring failures become `IO_FAILED` ([`JevError::IoFailed`]) with a
//! sanitized cause and a recovery hint. Upstream messages are dropped because `BadEncoding`
//! carries the raw secret bytes and platform errors can echo caller text.

use sc_agent_types::JevError;

use crate::resolve::{EnvSource, KeyringSource};

pub(crate) const KEYRING_UNREADABLE: &str = "platform keyring could not be read";

pub(crate) const KEYRING_RECOVERY: &str = "Unlock the platform keystore for this OS user and retry. Do not paste secret values into logs or this message.";

/// Reads variables from the process environment.
///
/// A missing variable is `None`. A present empty string is `Some("")`, so
/// repo-root `.env` does not override it. Non-Unicode values are treated as
/// present and empty.
#[derive(Debug, Default, Clone, Copy)]
pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        match std::env::var(name) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => Some(String::new()),
        }
    }
}

/// Reads secrets from the platform keyring.
///
/// On macOS and iOS this is the Keychain. On Windows it is Credential Manager.
/// On Linux and BSD it is the Secret Service. Other targets have no keystore
/// and lookups miss.
///
/// # Panics
///
/// Panics if the process-global keyring builder lock is poisoned. That poison
/// means another thread already panicked while building an entry.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlatformKeyring;

/// Longest a platform keyring read may take.
///
/// A keystore that waits for the user (a locked macOS Keychain with no way to
/// show its prompt, a Secret Service with no agent) can block a read forever.
/// A headless run must not hang on it, so the read gets a deadline and a
/// typed, retryable error (`IO_FAILED`, `timed_out`).
pub(crate) const KEYRING_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Runs `read` on its own thread and waits at most `timeout`.
///
/// On timeout the thread is left to finish (or stay blocked) on its own; it
/// only reads the keystore and holds no state, so abandoning it is safe, and
/// the process can still exit.
pub(crate) fn read_with_deadline<T: Send + 'static>(
    timeout: std::time::Duration,
    read: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(read());
    });
    receiver.recv_timeout(timeout).ok()
}

fn keyring_timed_out() -> JevError {
    JevError::io_failed_with(
        format!("{KEYRING_UNREADABLE} (it did not answer in time)"),
        std::io::ErrorKind::TimedOut,
        Some("Unlock or allow the platform keystore for this OS user and retry, or provide the key in the process environment or the repo-root .env file instead.".to_owned()),
    )
}

impl KeyringSource for PlatformKeyring {
    fn password(&self, service: &str, account: &str) -> Result<Option<String>, JevError> {
        let (service, account) = (service.to_owned(), account.to_owned());
        match read_with_deadline(KEYRING_READ_TIMEOUT, move || read_entry(&service, &account)) {
            Some(result) => result,
            None => Err(keyring_timed_out()),
        }
    }
}

fn read_entry(service: &str, account: &str) -> Result<Option<String>, JevError> {
    let entry = match keyring::Entry::new(service, account) {
        Ok(entry) => entry,
        Err(error) => {
            return Err(keyring_unavailable(
                sanitized_keyring_cause(&error),
                keyring_io_kind(&error),
            ));
        }
    };
    classify_keyring(entry.get_password())
}

pub(crate) fn classify_keyring(
    result: Result<String, keyring::Error>,
) -> Result<Option<String>, JevError> {
    match result {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(keyring_unavailable(
            sanitized_keyring_cause(&error),
            keyring_io_kind(&error),
        )),
    }
}

/// Labels a keyring failure without formatting the error.
///
/// `keyring::Error`'s `Display` and `Debug` can include secret bytes
/// (`BadEncoding`) or the inner platform message (`PlatformFailure`).
fn sanitized_keyring_cause(error: &keyring::Error) -> &'static str {
    match error {
        keyring::Error::PlatformFailure(_) => "platform keystore returned an error",
        keyring::Error::NoStorageAccess(_) => "keystore access was denied",
        keyring::Error::NoEntry => "entry is absent",
        keyring::Error::BadEncoding(_) => "stored value was not valid UTF-8",
        keyring::Error::TooLong(_, _) => "keystore rejected an attribute length",
        keyring::Error::Invalid(_, _) => "keystore rejected the service or account",
        keyring::Error::Ambiguous(_) => "keystore matched more than one entry",
        _ => "keystore returned an unrecognized error",
    }
}

fn keyring_unavailable(cause: &str, kind: std::io::ErrorKind) -> JevError {
    JevError::io_failed_with(
        format!("{KEYRING_UNREADABLE} ({cause})"),
        kind,
        Some(KEYRING_RECOVERY.to_owned()),
    )
}

/// The I/O class a keyring failure is reported as; the platform message is
/// never kept.
fn keyring_io_kind(error: &keyring::Error) -> std::io::ErrorKind {
    match error {
        keyring::Error::NoStorageAccess(_) => std::io::ErrorKind::PermissionDenied,
        keyring::Error::BadEncoding(_) => std::io::ErrorKind::InvalidData,
        _ => std::io::ErrorKind::Other,
    }
}

/// OS user name used as the keyring account.
#[must_use]
pub(crate) fn current_account() -> String {
    whoami::username()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_errors_drop_secret_bytes() {
        let secret = "super-secret-value";
        let cases = [
            (
                keyring::Error::BadEncoding(secret.as_bytes().to_vec()),
                "stored value was not valid UTF-8",
            ),
            (
                keyring::Error::PlatformFailure(Box::new(std::io::Error::other(secret))),
                "platform keystore returned an error",
            ),
            (
                keyring::Error::NoStorageAccess(Box::new(std::io::Error::other(secret))),
                "keystore access was denied",
            ),
            (
                keyring::Error::TooLong(secret.to_owned(), 8),
                "keystore rejected an attribute length",
            ),
            (
                keyring::Error::Invalid("password".to_owned(), secret.to_owned()),
                "keystore rejected the service or account",
            ),
        ];
        for (error, cause) in cases {
            let err = classify_keyring(Err(error)).expect_err("keyring failure");
            assert_eq!(err.code(), "IO_FAILED");
            let message = err.message();
            assert!(message.contains(KEYRING_UNREADABLE), "{message}");
            assert!(message.contains(cause), "{message}");
            let action = err.suggested_action();
            assert!(action.contains(KEYRING_RECOVERY), "{action}");
            assert!(!action.contains("check.toml"), "{action}");
            assert_secret_absent(&err, secret);
        }
        assert_eq!(
            classify_keyring(Err(keyring::Error::NoEntry)).unwrap(),
            None
        );
    }

    fn assert_secret_absent(error: &JevError, secret: &str) {
        let rendered = format!("{error} {error:?} {}", error.message());
        assert!(!rendered.contains(secret), "{rendered}");
    }

    /// A keystore read that never answers returns a typed, retryable error
    /// at the deadline instead of hanging (the macOS Keychain prompt case).
    #[test]
    fn a_blocked_keyring_read_times_out_with_a_typed_error() {
        let started = std::time::Instant::now();
        let blocked = read_with_deadline(std::time::Duration::from_millis(50), || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            1_u8
        });
        assert!(blocked.is_none());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let error = keyring_timed_out();
        assert_eq!(error.code(), "IO_FAILED");
        assert!(error.retryable());
        assert!(error.suggested_action().contains("process environment"));
        // A prompt answer is returned as is.
        assert_eq!(
            read_with_deadline(std::time::Duration::from_secs(5), || 7_u8),
            Some(7)
        );
    }
}
