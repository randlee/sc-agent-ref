//! The global model additions, `~/.sc-agent/models.toml` (agt-akp).
//!
//! New models are added here without recompiling:
//!
//! ```toml
//! [[model]]
//! id = "claude-haiku-5-6"
//! alias = "haiku"
//! released = "2026-12-01"
//! ```
//!
//! A short name resolves to the most recent `released` across [`MODELS`]
//! and this file. A row only extends an existing alias; a new alias cannot
//! create a backend. A malformed file is `CONFIG_INVALID`. The repo's
//! `check.toml` `[models]` still wins over it.
//!
//! The functions here take the path; the composition root
//! (`sc_jev_host::paths::models_file_with`) decides where the file lives, so
//! no test or caller needs a process-wide environment change.
//!
//! [`MODELS`]: sc_agent_types::MODELS

#![warn(missing_docs)]

use std::io::ErrorKind;
use std::path::Path;

use sc_agent_types::{JevError, ModelAliases};
use serde::Deserialize;

/// The catalog with the rows of the models file at `path`.
///
/// A missing file is not an error: the result is the crate table.
///
/// # Errors
///
/// `CONFIG_INVALID` for a malformed file (bad TOML, an unknown key, an
/// unknown alias, an invalid model id or date; the message names the file);
/// `IO_FAILED` when the file exists but cannot be read.
///
/// # Examples
///
/// ```
/// use sc_jev_check::models_file::models_from_str;
///
/// let aliases = models_from_str(
///     "[[model]]\nid = \"claude-haiku-9\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n",
/// )?;
/// assert_eq!(aliases.latest("haiku").unwrap().as_str(), "claude-haiku-9");
/// # Ok::<(), sc_agent_types::JevError>(())
/// ```
pub fn load_models_file(path: &Path) -> Result<ModelAliases, JevError> {
    match std::fs::read_to_string(path) {
        Ok(text) => models_from_str(&text).map_err(|error| {
            error.map_detail(|detail| {
                detail.replacen("models file", &format!("models file {}", path.display()), 1)
            })
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(ModelAliases::builtin()),
        Err(error) => Err(JevError::io_failed_with(
            format!("models file {}", path.display()),
            error.kind(),
            Some("Make the file readable or remove it.".to_owned()),
        )),
    }
}

/// [`load_models_file`] over the text of a file.
///
/// # Errors
///
/// `CONFIG_INVALID`, as [`load_models_file`].
pub fn models_from_str(text: &str) -> Result<ModelAliases, JevError> {
    let raw: RawFile = toml::from_str(text)
        .map_err(|error| JevError::config_invalid(format!("models file: {error}")))?;
    raw.model
        .iter()
        .try_fold(ModelAliases::builtin(), |aliases, row| {
            aliases.with_file_model(&row.id, &row.alias, &row.released)
        })
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    model: Vec<RawModel>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawModel {
    id: String,
    alias: String,
    released: String,
}

#[cfg(test)]
mod tests {
    use sc_agent_types::ModelSource;

    use super::*;

    #[test]
    fn newest_released_wins_across_crate_and_file() {
        let aliases = models_from_str(
            r#"
            [[model]]
            id = "claude-haiku-9"
            alias = "haiku"
            released = "2099-01-01"

            [[model]]
            id = "gpt-5-luna"
            alias = "luna"
            released = "2000-01-01"
            "#,
        )
        .expect("file");
        assert_eq!(
            aliases.latest("haiku").expect("m").as_str(),
            "claude-haiku-9"
        );
        assert_eq!(aliases.source("haiku"), Some(ModelSource::File));
        assert_eq!(aliases.latest("luna").expect("m").as_str(), "gpt-6-luna");
        assert_eq!(aliases.source("luna"), Some(ModelSource::Crate));
    }

    #[test]
    fn malformed_files_are_config_invalid() {
        for bad in [
            "[[model]]\nid = \"x\"\n",
            "[[model]]\nid = \"x\"\nalias = \"nope\"\nreleased = \"2099-01-01\"\n",
            "[[model]]\nid = \"x\"\nalias = \"haiku\"\nreleased = \"soon\"\n",
            "[[model]]\nid = \"\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n",
            "[[model]]\nid = \"x\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\nextra = 1\n",
            "[[model]]\nid = \"gpt-6-luna\"\nalias = \"haiku\"\nreleased = \"2099-01-01\"\n",
            "model = 3\n",
            "not toml [",
        ] {
            let error = models_from_str(bad).expect_err(bad);
            assert_eq!(error.code(), "CONFIG_INVALID", "{bad}");
        }
    }

    #[test]
    fn missing_file_is_the_crate_table_and_a_bad_file_names_its_path() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("models.toml");
        assert_eq!(
            load_models_file(&path).expect("absent"),
            ModelAliases::builtin()
        );
        std::fs::write(&path, "[[model]]\nid = 1\n").expect("write");
        let error = load_models_file(&path).expect_err("bad");
        assert_eq!(error.code(), "CONFIG_INVALID");
        assert!(
            error.message().contains("models.toml"),
            "{}",
            error.message()
        );
    }
}
