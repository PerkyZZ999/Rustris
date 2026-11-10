use crate::{Error, Result, bytes::OsBytes};
use std::collections::BTreeSet;
use uuid::Uuid;

pub const MAX_ARGUMENTS: usize = 1024;
pub const MAX_ENVIRONMENT: usize = 256;
pub const MAX_PROFILE_BYTES: usize = 1024 * 1024;

/// Supported profile kinds. Saving a kind is not proof its runner is installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runner {
    Native,
    Umu,
}

impl Runner {
    /// Stable persisted identifier.
    pub fn key(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Umu => "umu",
        }
    }

    /// Reject unknown persisted types without silently changing a profile.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "native" => Ok(Self::Native),
            "umu" => Ok(Self::Umu),
            _ => Err(Error::Invalid("Unrecognized profile runner.".into())),
        }
    }
}

/// Environment override: None explicitly unsets the key; Some(empty) sets empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvDelta {
    pub key: OsBytes,
    pub value: Option<OsBytes>,
}

/// A draft is validated before any transaction starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileInput {
    pub name: String,
    pub executable: OsBytes,
    pub cwd: OsBytes,
    pub arguments: Vec<OsBytes>,
    pub environment: Vec<EnvDelta>,
    pub runner: Runner,
    pub prefix: Option<OsBytes>,
}

impl ProfileInput {
    /// Syntactic validation only: unavailable games remain valid library entries.
    pub fn validate(&self) -> Result<()> {
        let payload = self.executable.as_bytes().len()
            + self.cwd.as_bytes().len()
            + self.prefix.as_ref().map_or(0, |p| p.as_bytes().len())
            + self
                .arguments
                .iter()
                .map(|a| a.as_bytes().len())
                .sum::<usize>()
            + self
                .environment
                .iter()
                .map(|e| {
                    e.key.as_bytes().len() + e.value.as_ref().map_or(0, |v| v.as_bytes().len())
                })
                .sum::<usize>();
        if self.arguments.len() > MAX_ARGUMENTS
            || self.environment.len() > MAX_ENVIRONMENT
            || payload > MAX_PROFILE_BYTES
        {
            return Err(Error::Invalid("A profile allows at most 1 MiB of values, 1024 arguments and 256 environment overrides.".into()));
        }
        if self.name.trim().is_empty()
            || self.name.chars().count() > 256
            || self.name.chars().any(char::is_control)
        {
            return Err(Error::Invalid(
                "Enter a game name with 1–256 characters and no control characters.".into(),
            ));
        }
        for (label, value) in [
            ("Executable", &self.executable),
            ("Working directory", &self.cwd),
        ] {
            if !value.path().is_absolute() {
                return Err(Error::Invalid(format!("{label} must be an absolute path.")));
            }
        }
        if self.runner == Runner::Umu && self.prefix.is_none() {
            return Err(Error::Invalid(
                "Choose a Wine-prefix path for an UMU profile.".into(),
            ));
        }
        if self
            .prefix
            .as_ref()
            .is_some_and(|p| !p.path().is_absolute())
        {
            return Err(Error::Invalid(
                "The prefix must be an absolute path.".into(),
            ));
        }
        let mut keys = BTreeSet::new();
        for delta in &self.environment {
            let key = delta.key.as_bytes();
            if key.is_empty() || key.contains(&b'=') || !keys.insert(key) {
                return Err(Error::Invalid(
                    "Environment keys must be nonempty, unique and contain no '='.".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Owned game, installation and profile remain separate identities.
#[derive(Clone, Debug)]
pub struct Entry {
    pub game_id: Uuid,
    pub installation_id: Uuid,
    pub profile_id: Uuid,
    pub revision: i64,
    pub favorite: bool,
    /// User-selected external file; the launcher never owns or deletes it.
    pub artwork: Option<OsBytes>,
    pub artwork_available: bool,
    /// D-010: a launch review was approved for this game; launching still
    /// re-checks that the reviewed details are unchanged.
    pub review_approved: bool,
    /// D-011: covers and banners Rustris downloaded into its own data folder.
    pub fetched_cover: Option<OsBytes>,
    pub fetched_hero: Option<OsBytes>,
    pub profile: ProfileInput,
    /// Observed on the worker, never used to delete or rewrite an installation.
    pub availability: String,
}

/// Stable-ID list filtering; paths and profile execution values are untouched.
pub fn filter(entries: &[Entry], query: &str, favorites: bool) -> Vec<usize> {
    let query = query.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            (!favorites || entry.favorite) && entry.profile.name.to_lowercase().contains(&query)
        })
        .map(|(index, _)| index)
        .collect()
}
