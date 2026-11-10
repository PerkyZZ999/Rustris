use super::Selection;
use crate::{Error, Result, bytes::OsBytes, paths::Paths, private_config};
use serde::{Deserialize, Serialize};
const NAME: &str = "runners.toml";
const LIMIT: u64 = 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    umu: String,
    proton: String,
    runtime: String,
}
impl Paths {
    pub fn read_runner_selection(&self) -> Result<Option<Selection>> {
        let Some(text) = private_config::read(&self.config, NAME, LIMIT)? else {
            return Ok(None);
        };
        let document:Document=toml::from_str(&text).map_err(|_|Error::Invalid("runners.toml is invalid or unsupported. Preserve it before repair; it was not replaced.".into()))?;
        if document.version != 1 {
            return Err(Error::Invalid(
                "The runner selection version is unsupported; runners.toml was preserved.".into(),
            ));
        }
        let selection = Selection {
            executable: OsBytes::from_editor(&document.umu, true)?,
            proton: OsBytes::from_editor(&document.proton, true)?,
            runtime: OsBytes::from_editor(&document.runtime, true)?,
        };
        validate(&selection)?;
        Ok(Some(selection))
    }
    /// Store selection only. Availability/version collection is an independent worker operation.
    pub fn write_runner_selection(&self, selection: &Selection) -> Result<()> {
        validate(selection)?;
        self.read_runner_selection()?;
        let text = toml::to_string(&Document {
            version: 1,
            umu: selection.executable.escaped(),
            proton: selection.proton.escaped(),
            runtime: selection.runtime.escaped(),
        })
        .map_err(|e| Error::Invalid(e.to_string()))?;
        if text.len() as u64 > LIMIT {
            return Err(Error::Invalid(
                "The encoded runner selection exceeds 1 MiB.".into(),
            ));
        }
        private_config::replace(&self.config, NAME, &text)
    }
}
fn validate(selection: &Selection) -> Result<()> {
    for (label, value) in [
        ("UMU executable", &selection.executable),
        ("Proton directory", &selection.proton),
        ("Prepared runtime directory", &selection.runtime),
    ] {
        if !value.path().is_absolute() {
            return Err(Error::Invalid(format!(
                "{label} must be a full path starting with /, for example /usr/bin/umu-run."
            )));
        }
    }
    Ok(())
}
