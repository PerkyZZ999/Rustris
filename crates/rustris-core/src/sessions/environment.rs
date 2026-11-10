use super::*;
use crate::domain::{EnvDelta, ProfileInput};
use std::{collections::BTreeMap, os::unix::ffi::OsStrExt};

/// Deliberately selected ambient values; launcher credentials/internal variables are absent.
#[derive(Clone, Default)]
pub struct BaseEnvironment(Vec<(OsBytes, OsBytes)>);
impl BaseEnvironment {
    pub fn capture() -> Result<Self> {
        Self::from_pairs(std::env::vars_os().map(|(key, value)| {
            (
                key.as_os_str().as_bytes().to_vec(),
                value.as_os_str().as_bytes().to_vec(),
            )
        }))
    }
    /// Also permits deterministic injected environments without mutating process globals.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>) -> Result<Self> {
        let mut selected = BTreeMap::new();
        for (key, value) in pairs {
            let Ok(name) = std::str::from_utf8(&key) else {
                continue;
            };
            if ambient_allowed(name) {
                selected.insert(key, OsBytes::new(value)?);
            }
        }
        Ok(Self(
            selected
                .into_iter()
                .map(|(key, value)| Ok((OsBytes::new(key)?, value)))
                .collect::<Result<_>>()?,
        ))
    }
    pub(crate) fn effective(&self, profile: &ProfileInput) -> Result<Vec<(OsBytes, OsBytes)>> {
        let mut values: BTreeMap<Vec<u8>, OsBytes> = self
            .0
            .iter()
            .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
            .collect();
        for delta in &profile.environment {
            if let Some(value) = &delta.value {
                if sensitive_key(delta.key.as_bytes()) {
                    return Err(Error::Invalid("This profile sets a credential-like or internal variable. Remove it before launching; S2 does not persist credentials in launch snapshots.".into()));
                }
                values.insert(delta.key.as_bytes().to_vec(), value.clone());
            } else {
                values.remove(delta.key.as_bytes());
            }
        }
        let environment: Vec<_> = values
            .into_iter()
            .map(|(key, value)| Ok((OsBytes::new(key)?, value)))
            .collect::<Result<_>>()?;
        let mut effective = profile.clone();
        effective.environment = environment
            .iter()
            .map(|(key, value)| EnvDelta {
                key: key.clone(),
                value: Some(value.clone()),
            })
            .collect();
        effective.validate()?;
        Ok(environment)
    }
}
fn ambient_allowed(name: &str) -> bool {
    matches!(
        name,
        "HOME"
            | "USER"
            | "LOGNAME"
            | "PATH"
            | "LANG"
            | "LANGUAGE"
            | "DISPLAY"
            | "WAYLAND_DISPLAY"
            | "XAUTHORITY"
            | "XDG_RUNTIME_DIR"
            | "DBUS_SESSION_BUS_ADDRESS"
            | "XDG_CONFIG_HOME"
            | "XDG_DATA_HOME"
            | "XDG_CACHE_HOME"
            | "XDG_STATE_HOME"
            | "XDG_CURRENT_DESKTOP"
            | "XDG_SESSION_TYPE"
            | "XDG_SESSION_DESKTOP"
            | "DESKTOP_SESSION"
            | "PULSE_SERVER"
            | "PIPEWIRE_REMOTE"
            | "LC_ALL"
            | "LC_CTYPE"
            | "LC_NUMERIC"
            | "LC_TIME"
            | "LC_COLLATE"
            | "LC_MONETARY"
            | "LC_MESSAGES"
            | "LC_PAPER"
            | "LC_NAME"
            | "LC_ADDRESS"
            | "LC_TELEPHONE"
            | "LC_MEASUREMENT"
            | "LC_IDENTIFICATION"
    )
}
fn sensitive_key(key: &[u8]) -> bool {
    let name = String::from_utf8_lossy(key).to_ascii_uppercase();
    name.starts_with("RUSTRIS_")
        || [
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "PASSWD",
            "API_KEY",
            "PRIVATE_KEY",
            "CREDENTIAL",
        ]
        .iter()
        .any(|part| name.contains(part))
}
