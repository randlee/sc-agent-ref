//! Credential lookup: process env, repo-root `.env`, then keyring.

#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::fmt;

use sc_agent_types::observability::v2::{GeneralEvent, GeneralLog, NoopGeneralLog};
use sc_agent_types::{BackendKeyId, JevError, RepoRoot, ResolvedApiKey};

/// Logging target of this module.
const TARGET: &str = "secrets.resolve";

use crate::dotenv::load_dotenv;
use crate::platform::current_account;

/// Environment variable and default keyring service for TypeSafe.
pub const TYPESAFE_API_KEY_VAR: &str = "TYPESAFE_API_KEY";

/// Environment variable and default keyring service for Anthropic.
pub const ANTHROPIC_API_KEY_VAR: &str = "ANTHROPIC_API_KEY";

/// Environment variable and default keyring service for OpenAI.
pub const OPENAI_API_KEY_VAR: &str = "OPENAI_API_KEY";

/// Optional Anthropic keyring service override.
///
/// When this process environment variable is set to a non-empty value,
/// Anthropic keyring lookups use it as the service name. A repo-root `.env`
/// is never consulted for it. It does not change the Anthropic environment
/// variable, and other backends ignore it.
pub const ANTHROPIC_KEYCHAIN_SERVICE_VAR: &str = "ANTHROPIC_KEYCHAIN_SERVICE";

/// Reads named variables.
///
/// `None` means unset. `Some` means set, including an empty string. The trait
/// is an open extension point (ADR-SAG-0020) so tests can inject values
/// without changing the process environment. It is intentionally not sealed.
pub trait EnvSource: Send + Sync {
    /// Returns the variable named `name` when it is set.
    fn var(&self, name: &str) -> Option<String>;
}

/// Reads one keyring entry.
///
/// `Ok(None)` means the entry is absent. The trait is an open extension point
/// (ADR-SAG-0020) so tests can inject a store. It is intentionally not sealed.
/// Implementations must not put secret material in errors. A store that cannot
/// be read returns [`JevError::io_failed_with`] (`IO_FAILED`) with a
/// sanitized cause in `what`.
pub trait KeyringSource: Send + Sync {
    /// Returns the secret for `service` and `account`.
    ///
    /// # Errors
    ///
    /// Returns [`JevError::IoFailed`] when the store cannot be read.
    /// The error must not include secret bytes or passwords.
    fn password(&self, service: &str, account: &str) -> Result<Option<String>, JevError>;
}

/// Resolves one backend API key.
///
/// Search order is the process environment, then `repo_root/.env`, then the
/// platform keyring. `.env` does not override a variable that is already set.
/// The keyring service is the variable name. The account is the OS user.
/// Anthropic keyring lookups honor [`ANTHROPIC_KEYCHAIN_SERVICE_VAR`] from the
/// process environment only.
/// The returned [`ResolvedApiKey`] redacts the secret in `Debug` and `Display`.
///
/// This function does not run git and does not contact the network.
///
/// # Errors
///
/// Returns [`JevError::KeyMissing`] (`KEY_MISSING`) when every source lacks a
/// non-empty key. Returns [`JevError::IoFailed`] (`IO_FAILED`) when `.env` or
/// the keyring cannot be read, and [`JevError::CredentialStoreInvalid`]
/// (`CONFIG_INVALID`) when the first non-empty key contains interior
/// whitespace or a control character ([`ResolvedApiKey::try_new`]). Each
/// carries a sanitized cause and a recovery step. No error includes secret
/// material.
///
/// # Panics
///
/// Panics if the process-global keyring builder lock is poisoned.
///
/// # Examples
///
/// ```no_run
/// use sc_agent_secrets::resolve_key;
/// use sc_agent_types::{BackendKeyId, RepoRoot};
///
/// let root = RepoRoot::new(".");
/// let key = resolve_key(BackendKeyId::TypeSafe, &root)?;
/// assert_eq!(key.to_string(), "[redacted]");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
pub fn resolve_key(id: BackendKeyId, repo_root: &RepoRoot) -> Result<ResolvedApiKey, JevError> {
    resolve_key_logged(id, repo_root, &NoopGeneralLog)
}

/// [`resolve_key`] that reports which source supplied the key to `log`.
///
/// The general log records the source (`env`, `dotenv` or `keyring`) and the
/// variable name, never the value (NFR-SAG-0002). Logging is best effort and
/// never changes the result.
///
/// # Errors
///
/// As [`resolve_key`].
///
/// # Panics
///
/// Panics if the process-global keyring builder lock is poisoned.
pub fn resolve_key_logged(
    id: BackendKeyId,
    repo_root: &RepoRoot,
    log: &dyn GeneralLog,
) -> Result<ResolvedApiKey, JevError> {
    resolve_key_with_logged(
        id,
        repo_root,
        &crate::ProcessEnv,
        &crate::PlatformKeyring,
        current_account(),
        log,
    )
}

/// Resolves one backend API key from injected sources.
///
/// `account` is the keyring user. Precedence matches [`resolve_key`]. Passing
/// an empty account skips the keyring and yields `KEY_MISSING` when no earlier
/// source has a key.
///
/// # Errors
///
/// Returns [`JevError::KeyMissing`] when no source has a non-empty key.
/// Returns [`JevError::IoFailed`] when `.env` or `keyring` cannot be
/// read, and [`JevError::CredentialStoreInvalid`] when the selected key fails
/// [`ResolvedApiKey::try_new`]. Each names a sanitized cause and a recovery
/// step. Error text
/// does not include secret material.
///
/// # Examples
///
/// See the crate documentation for an injected miss that returns `KEY_MISSING`.
pub fn resolve_key_with(
    id: BackendKeyId,
    repo_root: &RepoRoot,
    env: &impl EnvSource,
    keyring: &impl KeyringSource,
    account: impl AsRef<str>,
) -> Result<ResolvedApiKey, JevError> {
    resolve_key_with_logged(id, repo_root, env, keyring, account, &NoopGeneralLog)
}

/// [`resolve_key_with`] that reports the winning source to `log`.
///
/// # Errors
///
/// As [`resolve_key_with`].
pub fn resolve_key_with_logged(
    id: BackendKeyId,
    repo_root: &RepoRoot,
    env: &impl EnvSource,
    keyring: &impl KeyringSource,
    account: impl AsRef<str>,
    log: &dyn GeneralLog,
) -> Result<ResolvedApiKey, JevError> {
    let resolved = resolve_in_order(id, repo_root, env, keyring, account.as_ref());
    match &resolved {
        Ok((_, source)) => {
            GeneralEvent::debug(TARGET, "credential.resolved", "credential resolved")
                .with_field("key_id", id.to_string())
                .with_field("source", *source)
                .emit(log)
        }
        Err(error) => GeneralEvent::warn(TARGET, "credential.failed", "credential not resolved")
            .with_field("key_id", id.to_string())
            .with_error_code(error.code().as_str())
            .emit(log),
    }
    resolved.map(|(key, _)| key)
}

/// The source that supplied a key, for the general log.
type Source = &'static str;

fn resolve_in_order(
    id: BackendKeyId,
    repo_root: &RepoRoot,
    env: &impl EnvSource,
    keyring: &impl KeyringSource,
    account: &str,
) -> Result<(ResolvedApiKey, Source), JevError> {
    let Some(var_name) = env_var_name(id) else {
        return Err(missing(id));
    };
    let process_value = env.var(var_name);
    if let Some(raw) = process_value.clone().filter(|value| is_present(value)) {
        return ResolvedApiKey::try_new(id, raw).map(|key| (key, "env"));
    }

    if process_value.is_none() {
        let dotenv = load_dotenv(repo_root.as_path())?;
        if let Some(raw) = dotenv.get(var_name).filter(|value| is_present(value)) {
            return ResolvedApiKey::try_new(id, raw.clone()).map(|key| (key, "dotenv"));
        }
    }

    let service = keyring_service(id, env);
    if account.is_empty() {
        return Err(missing(id));
    }
    let stored = keyring
        .password(&service, account)
        .map_err(|error| name_key_variable(error, var_name))?;
    match stored {
        Some(raw) if is_present(&raw) => {
            ResolvedApiKey::try_new(id, raw).map(|key| (key, "keyring"))
        }
        _ => Err(missing(id)),
    }
}

/// Returns the environment variable name for `id`.
#[must_use]
pub(crate) fn env_var_name(id: BackendKeyId) -> Option<&'static str> {
    KEY_ENV_VARS
        .iter()
        .find(|(key, _)| *key == id)
        .map(|(_, name)| *name)
}

/// Environment variable of every credential this crate resolves.
const KEY_ENV_VARS: &[(BackendKeyId, &str)] = &[
    (BackendKeyId::TypeSafe, TYPESAFE_API_KEY_VAR),
    (BackendKeyId::Anthropic, ANTHROPIC_API_KEY_VAR),
    (BackendKeyId::OpenAi, OPENAI_API_KEY_VAR),
];

/// The keyring service for `id`.
///
/// The Anthropic override is read from the process environment only. A repo
/// `.env` is repository content, and letting it pick which keychain item is
/// sent to Anthropic as the key would let a checkout choose which stored
/// secret leaves the machine.
fn keyring_service(id: BackendKeyId, env: &impl EnvSource) -> String {
    if id == BackendKeyId::Anthropic
        && let Some(service) = env
            .var(ANTHROPIC_KEYCHAIN_SERVICE_VAR)
            .and_then(|value| usable_secret(&value))
    {
        return service;
    }
    env_var_name(id).unwrap_or_default().to_owned()
}

/// Whether a source holds a value worth sealing.
///
/// A blank `.env` or keyring value counts as absent, so the next source is
/// tried. A blank process-environment value is set, not absent: it skips the
/// `.env` file and goes to the keyring. Everything else goes to
/// [`ResolvedApiKey::try_new`], which owns the normalization rules.
fn is_present(value: &str) -> bool {
    !value.trim().is_empty()
}

fn usable_secret(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// Adds the variable name to the recovery step of a keyring read failure.
///
/// The keyring adapter knows the service, not which variable a user can set
/// instead, and a headless machine with no keystore needs that answer more
/// than keystore advice. Error id, kind and retryability are unchanged.
fn name_key_variable(error: JevError, var_name: &str) -> JevError {
    match error {
        JevError::IoFailed {
            what,
            kind,
            recovery: Some(recovery),
            ..
        } => JevError::io_failed_with(
            what,
            kind,
            Some(format!(
                "{recovery} Or set {var_name} in the process environment or in the repo-root .env file; the keystore is then not read."
            )),
        ),
        other => other,
    }
}

fn missing(id: BackendKeyId) -> JevError {
    JevError::key_missing(id)
}

/// In-memory environment map for tests.
///
/// [`Debug`] prints variable names and omits values.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct MapEnv {
    vars: BTreeMap<String, String>,
}

#[cfg(test)]
impl MapEnv {
    /// Builds a map from `(name, value)` pairs.
    #[must_use]
    pub fn with<K, V, I>(pairs: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        Self {
            vars: pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }
}

#[cfg(test)]
impl fmt::Debug for MapEnv {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MapEnv")
            .field("names", &self.vars.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
impl EnvSource for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }
}

/// In-memory keyring for tests.
///
/// Entries are visible only for the account given at construction. [`Debug`]
/// prints the account and service names and omits secrets.
#[cfg(test)]
#[derive(Clone)]
pub struct MapKeyring {
    account: String,
    entries: BTreeMap<String, String>,
}

#[cfg(test)]
impl MapKeyring {
    /// Builds a store whose secrets are returned only for `account`.
    #[must_use]
    pub fn new<S, V, I>(account: impl Into<String>, entries: I) -> Self
    where
        S: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (S, V)>,
    {
        Self {
            account: account.into(),
            entries: entries
                .into_iter()
                .map(|(service, secret)| (service.into(), secret.into()))
                .collect(),
        }
    }
}

#[cfg(test)]
impl fmt::Debug for MapKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MapKeyring")
            .field("account", &self.account)
            .field("services", &self.entries.keys().collect::<Vec<_>>())
            .finish()
    }
}

#[cfg(test)]
impl KeyringSource for MapKeyring {
    fn password(&self, service: &str, account: &str) -> Result<Option<String>, JevError> {
        if account != self.account {
            return Ok(None);
        }
        Ok(self.entries.get(service).cloned())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::dotenv::{DOTENV_RECOVERY, DOTENV_UNREADABLE};
    use crate::platform::{KEYRING_RECOVERY, KEYRING_UNREADABLE};
    use crate::test_support::TempRepo;
    use sc_agent_types::ResolvedApiKey;

    const SECRET: &str = "super-secret-value";

    /// Expected key, compared by value so tests never read the secret.
    fn sealed(secret: &str) -> ResolvedApiKey {
        ResolvedApiKey::try_new(BackendKeyId::TypeSafe, secret.to_owned()).expect("test key")
    }

    fn assert_no_secret(error: &JevError, secret: &str) {
        let rendered = format!("{error} {error:?} {}", error.message());
        assert!(!rendered.contains(secret), "{rendered}");
        assert!(!rendered.to_ascii_lowercase().contains("bearer "));
    }

    #[test]
    fn test_api_key_env_first_then_keychain() {
        let repo = TempRepo::new(Some("OPENAI_API_KEY=from-dotenv\n"));
        let env = MapEnv::with([("OPENAI_API_KEY", "from-env")]);
        let keyring = MapKeyring::new("tester", [("OPENAI_API_KEY", "from-keyring")]);
        let key =
            resolve_key_with(BackendKeyId::OpenAi, &repo.root(), &env, &keyring, "tester").unwrap();
        assert_eq!(key, sealed("from-env"));
    }

    #[test]
    fn test_jev_key_env_first_then_keychain() {
        let repo = TempRepo::new(Some("TYPESAFE_API_KEY=from-dotenv\n"));
        let env = MapEnv::with([("TYPESAFE_API_KEY", "from-env")]);
        let keyring = MapKeyring::new("tester", [("TYPESAFE_API_KEY", "from-keyring")]);
        let key = resolve_key_with(
            BackendKeyId::TypeSafe,
            &repo.root(),
            &env,
            &keyring,
            "tester",
        )
        .unwrap();
        assert_eq!(key, sealed("from-env"));
    }

    #[test]
    fn test_api_key_missing_sends_nothing() {
        let repo = TempRepo::new(Some(&format!("OTHER={SECRET}\n")));
        let env = MapEnv::with([("UNRELATED_SECRET", SECRET)]);
        let keyring = MapKeyring::new("tester", [("OTHER_SERVICE", SECRET)]);
        let error = resolve_key_with(
            BackendKeyId::Anthropic,
            &repo.root(),
            &env,
            &keyring,
            "tester",
        )
        .unwrap_err();
        assert_eq!(error.code(), "KEY_MISSING");
        assert!(error.suggested_action().contains("keyring"));
        assert_no_secret(&error, SECRET);
        assert!(matches!(
            error,
            JevError::KeyMissing {
                backend: BackendKeyId::Anthropic,
                ..
            }
        ));
    }

    #[test]
    fn blank_process_env_blocks_dotenv_and_uses_keyring() {
        let repo = TempRepo::new(Some("TYPESAFE_API_KEY=from-dotenv\n"));
        let env = MapEnv::with([("TYPESAFE_API_KEY", "  ")]);
        let keyring = MapKeyring::new("tester", [("TYPESAFE_API_KEY", "from-keyring")]);
        let key = resolve_key_with(
            BackendKeyId::TypeSafe,
            &repo.root(),
            &env,
            &keyring,
            "tester",
        )
        .unwrap();
        assert_eq!(key, sealed("from-keyring"));
    }

    #[test]
    fn anthropic_keychain_service_override_is_anthropic_only() {
        let repo = TempRepo::new(None);
        let env = MapEnv::with([("ANTHROPIC_KEYCHAIN_SERVICE", "custom-service")]);
        let keyring = MapKeyring::new(
            "tester",
            [
                ("custom-service", "from-custom"),
                ("ANTHROPIC_API_KEY", "from-default"),
                ("OPENAI_API_KEY", "from-openai"),
            ],
        );
        let anthropic = resolve_key_with(
            BackendKeyId::Anthropic,
            &repo.root(),
            &env,
            &keyring,
            "tester",
        )
        .unwrap();
        assert_eq!(anthropic, sealed("from-custom"));
        let openai =
            resolve_key_with(BackendKeyId::OpenAi, &repo.root(), &env, &keyring, "tester").unwrap();
        assert_eq!(openai, sealed("from-openai"));
    }

    #[test]
    fn anthropic_override_in_dotenv_is_ignored() {
        let repo = TempRepo::new(Some("ANTHROPIC_KEYCHAIN_SERVICE=from-dotenv-service\n"));
        let keyring = MapKeyring::new(
            "alice",
            [
                ("from-dotenv-service", "from-custom"),
                ("ANTHROPIC_API_KEY", "from-default"),
            ],
        );
        let key = resolve_key_with(
            BackendKeyId::Anthropic,
            &repo.root(),
            &MapEnv::default(),
            &keyring,
            "alice",
        )
        .unwrap();
        assert_eq!(key, sealed("from-default"));
    }

    #[test]
    fn process_override_is_used_and_dotenv_override_is_not() {
        let repo = TempRepo::new(Some("ANTHROPIC_KEYCHAIN_SERVICE=from-dotenv-service\n"));
        let env = MapEnv::with([("ANTHROPIC_KEYCHAIN_SERVICE", "from-env-service")]);
        let keyring = MapKeyring::new(
            "tester",
            [
                ("from-env-service", "from-env-ring"),
                ("from-dotenv-service", "from-dotenv-ring"),
            ],
        );
        let key = resolve_key_with(
            BackendKeyId::Anthropic,
            &repo.root(),
            &env,
            &keyring,
            "tester",
        )
        .unwrap();
        assert_eq!(key, sealed("from-env-ring"));
    }

    #[test]
    fn wrong_keyring_account_misses() {
        let repo = TempRepo::new(None);
        let keyring = MapKeyring::new("alice", [("OPENAI_API_KEY", "from-keyring")]);
        let error = resolve_key_with(
            BackendKeyId::OpenAi,
            &repo.root(),
            &MapEnv::default(),
            &keyring,
            "bob",
        )
        .unwrap_err();
        assert_eq!(error.code(), "KEY_MISSING");
        assert_no_secret(&error, "from-keyring");
    }

    #[test]
    fn unreadable_dotenv_is_io_failed_without_secrets() {
        let repo = TempRepo::new(None);
        fs::create_dir(repo.path().join(".env")).unwrap();
        let env = MapEnv::with([("UNRELATED_SECRET", SECRET)]);
        let keyring = MapKeyring::new("tester", [("OPENAI_API_KEY", SECRET)]);
        let error = resolve_key_with(BackendKeyId::OpenAi, &repo.root(), &env, &keyring, "tester")
            .unwrap_err();
        assert_eq!(error.code(), "IO_FAILED");
        let message = error.message();
        assert!(message.contains(DOTENV_UNREADABLE), "{message}");
        assert!(error.suggested_action().contains(DOTENV_RECOVERY));
        assert!(!error.suggested_action().contains("check.toml"));
        assert_no_secret(&error, SECRET);
    }

    #[test]
    fn process_env_key_ignores_unreadable_dotenv() {
        let repo = TempRepo::new(None);
        fs::create_dir(repo.path().join(".env")).unwrap();
        let env = MapEnv::with([("OPENAI_API_KEY", "from-env")]);
        let keyring = MapKeyring::new("tester", Vec::<(&str, &str)>::new());
        let key =
            resolve_key_with(BackendKeyId::OpenAi, &repo.root(), &env, &keyring, "tester").unwrap();
        assert_eq!(key, sealed("from-env"));
    }

    #[test]
    fn keyring_failure_does_not_echo_secrets() {
        struct Unavailable;
        impl KeyringSource for Unavailable {
            fn password(&self, service: &str, _account: &str) -> Result<Option<String>, JevError> {
                let _ = service;
                Err(JevError::io_failed_with(
                    format!("{KEYRING_UNREADABLE} (platform keystore returned an error)"),
                    std::io::ErrorKind::Other,
                    Some(KEYRING_RECOVERY.to_owned()),
                ))
            }
        }
        let repo = TempRepo::new(None);
        let error = resolve_key_with(
            BackendKeyId::TypeSafe,
            &repo.root(),
            &MapEnv::default(),
            &Unavailable,
            "tester",
        )
        .unwrap_err();
        assert_eq!(error.code(), "IO_FAILED");
        let hint = error.message();
        assert!(hint.contains(KEYRING_UNREADABLE), "{hint}");
        assert!(
            hint.contains("platform keystore returned an error"),
            "{hint}"
        );
        assert!(error.suggested_action().contains(KEYRING_RECOVERY));
        assert!(!error.suggested_action().contains("check.toml"));
        assert!(!hint.contains(SECRET), "{hint}");
        assert_no_secret(&error, SECRET);
    }

    /// A keystore failure still tells the user which variable to set.
    #[test]
    fn keyring_failure_names_the_backend_variable() {
        struct Unavailable;
        impl KeyringSource for Unavailable {
            fn password(&self, _service: &str, _account: &str) -> Result<Option<String>, JevError> {
                Err(JevError::io_failed_with(
                    format!("{KEYRING_UNREADABLE} (platform keystore returned an error)"),
                    std::io::ErrorKind::Other,
                    Some(KEYRING_RECOVERY.to_owned()),
                ))
            }
        }
        let repo = TempRepo::new(None);
        for (id, var) in [
            (BackendKeyId::TypeSafe, "TYPESAFE_API_KEY"),
            (BackendKeyId::Anthropic, "ANTHROPIC_API_KEY"),
            (BackendKeyId::OpenAi, "OPENAI_API_KEY"),
        ] {
            let error = resolve_key_with(id, &repo.root(), &MapEnv::default(), &Unavailable, "u")
                .unwrap_err();
            assert_eq!(error.code(), "IO_FAILED");
            assert!(!error.retryable());
            assert_eq!(
                error.suggested_action(),
                format!(
                    "{KEYRING_RECOVERY} Or set {var} in the process environment or in the repo-root .env file; the keystore is then not read."
                )
            );
        }
    }

    #[test]
    fn debug_omits_secret_values() {
        let env = MapEnv::with([("TYPESAFE_API_KEY", SECRET)]);
        let rendered = format!("{env:?}");
        assert!(rendered.contains("TYPESAFE_API_KEY"));
        assert!(!rendered.contains(SECRET));

        let keyring = MapKeyring::new("tester", [("OPENAI_API_KEY", SECRET)]);
        let rendered = format!("{keyring:?}");
        assert!(rendered.contains("OPENAI_API_KEY"));
        assert!(rendered.contains("tester"));
        assert!(!rendered.contains(SECRET));

        let resolved = sealed(SECRET);
        let rendered = format!("{resolved:?} {resolved}");
        assert!(rendered.contains("[redacted]"));
        assert!(!rendered.contains(SECRET));
        assert_eq!(resolved, sealed(SECRET));
    }

    #[test]
    fn keys_are_normalized_once_by_try_new() {
        let repo = TempRepo::new(Some("OPENAI_API_KEY=  from-dotenv  \n"));
        let keyring = MapKeyring::new("tester", [("TYPESAFE_API_KEY", " from-keyring\n")]);
        let trimmed = resolve_key_with(
            BackendKeyId::OpenAi,
            &repo.root(),
            &MapEnv::default(),
            &keyring,
            "tester",
        )
        .unwrap();
        assert_eq!(trimmed, sealed("from-dotenv"));
        let from_ring = resolve_key_with(
            BackendKeyId::TypeSafe,
            &repo.root(),
            &MapEnv::default(),
            &keyring,
            "tester",
        )
        .unwrap();
        assert_eq!(from_ring, sealed("from-keyring"));

        for (source_env, source_dotenv) in [
            (Some(format!("{SECRET} pasted twice")), None),
            (None, Some(format!("ANTHROPIC_API_KEY={SECRET}\tx\n"))),
        ] {
            let repo = TempRepo::new(source_dotenv.as_deref());
            let env = MapEnv::with(
                source_env
                    .into_iter()
                    .map(|value| ("ANTHROPIC_API_KEY".to_owned(), value)),
            );
            let error = resolve_key_with(
                BackendKeyId::Anthropic,
                &repo.root(),
                &env,
                &MapKeyring::new("tester", Vec::<(&str, &str)>::new()),
                "tester",
            )
            .unwrap_err();
            assert_eq!(error.code(), "CONFIG_INVALID");
            assert!(
                error
                    .message()
                    .contains("whitespace or a control character")
            );
            assert_no_secret(&error, SECRET);
        }
    }

    #[test]
    fn sources_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MapEnv>();
        assert_send_sync::<MapKeyring>();
        assert_send_sync::<crate::ProcessEnv>();
        assert_send_sync::<crate::PlatformKeyring>();
    }

    #[test]
    fn resolution_does_not_mutate_process_env() {
        let name = "SC_AGENT_SECRETS_RESOLVE_PROBE";
        let before = std::env::var(name);
        let repo = TempRepo::new(Some(&format!(
            "{name}=from-dotenv\nOPENAI_API_KEY=from-dotenv\n"
        )));
        let key = resolve_key_with(
            BackendKeyId::OpenAi,
            &repo.root(),
            &MapEnv::default(),
            &MapKeyring::new("tester", Vec::<(&str, &str)>::new()),
            "tester",
        )
        .unwrap();
        assert_eq!(key, sealed("from-dotenv"));
        assert_eq!(std::env::var(name).ok(), before.ok());
    }
}
