use crate::{Error, Result, error::io};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
};

/// App-owned locations, resolved without filesystem I/O.
#[derive(Clone, Debug)]
pub struct Paths {
    pub data: PathBuf,
    pub config: PathBuf,
}

impl Paths {
    /// Honor byte-safe XDG overrides, with standard home-directory fallbacks.
    pub fn discover() -> Result<Self> {
        Self::from_environment(
            env::var_os("HOME"),
            env::var_os("XDG_DATA_HOME"),
            env::var_os("XDG_CONFIG_HOME"),
        )
    }

    fn from_environment(
        home: Option<std::ffi::OsString>,
        data: Option<std::ffi::OsString>,
        config: Option<std::ffi::OsString>,
    ) -> Result<Self> {
        let location = |key, value: Option<std::ffi::OsString>, fallback| {
            let root = if let Some(value) = value.filter(|value| !value.is_empty()) {
                PathBuf::from(value)
            } else {
                let home = home
                    .as_ref()
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .ok_or_else(|| {
                        Error::Invalid("An absolute HOME is needed for unset XDG paths.".into())
                    })?;
                home.join(fallback)
            };
            if !root.is_absolute() {
                return Err(Error::Invalid(format!(
                    "{key} must be an absolute directory."
                )));
            }
            Ok(root.join("rustris"))
        };
        Ok(Self {
            data: location("XDG_DATA_HOME", data, ".local/share")?,
            config: location("XDG_CONFIG_HOME", config, ".config")?,
        })
    }

    /// Explicit private roots for isolated tools/tests; never a game installation.
    pub fn in_root(root: &Path) -> Self {
        Self {
            data: root.join("data"),
            config: root.join("config"),
        }
    }

    pub fn database(&self) -> PathBuf {
        self.data.join("library.sqlite3")
    }
    pub fn preferences(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    pub fn prepare(&self) -> Result<()> {
        private_directory(&self.data)?;
        private_directory(&self.config)
    }

    pub fn read_settings(&self) -> Result<Settings> {
        let Some(text) = crate::private_config::read(&self.config, "config.toml", 64 * 1024)?
        else {
            return Ok(Settings::default());
        };
        toml::from_str(&text).map_err(|_|Error::Invalid("Preferences could not be read. Repair or back up config.toml before changing them; defaults are shown.".into()))
    }
    pub fn write_settings(&self, settings: Settings) -> Result<()> {
        self.read_settings()?;
        let text = toml::to_string(&settings).map_err(|e| Error::Invalid(e.to_string()))?;
        crate::private_config::replace(&self.config, "config.toml", &text)
    }
}

/// Non-execution preferences, separate from raw profile bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub opaque: bool,
    pub reduced_motion: bool,
}

pub fn private_directory(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|e| io("create private app directories", e))?;
    }
    verify_private_directory(path)
}

pub(crate) fn verify_private_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| io("inspect private app directories", e))?;
    if !meta.is_dir()
        || meta.uid() != rustix::process::geteuid().as_raw()
        || meta.mode() & 0o077 != 0
    {
        return Err(Error::Invalid(format!(
            "The app directory {} must be owned by you, not a symlink, and private (0700).",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Explicit fixture assertions")]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    #[test]
    fn xdg_roots_preserve_bytes_and_do_not_require_home_when_explicit() {
        let data = OsString::from_vec(b"/private/data_\xff".to_vec());
        let paths =
            Paths::from_environment(None, Some(data.clone()), Some("/private/config".into()))
                .unwrap();
        assert_eq!(paths.data, PathBuf::from(data).join("rustris"));
        assert_eq!(paths.config, PathBuf::from("/private/config/rustris"));
    }

    #[test]
    fn empty_overrides_use_home_and_relative_locations_fail() {
        let paths =
            Paths::from_environment(Some("/private/home".into()), Some("".into()), None).unwrap();
        assert_eq!(
            paths.data,
            PathBuf::from("/private/home/.local/share/rustris")
        );
        assert_eq!(paths.config, PathBuf::from("/private/home/.config/rustris"));
        assert!(Paths::from_environment(None, None, None).is_err());
        assert!(
            Paths::from_environment(Some("/home/test".into()), Some("relative".into()), None)
                .is_err()
        );
    }
}
