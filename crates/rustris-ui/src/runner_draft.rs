use rustris_core::{Error, Result, bytes::OsBytes, runners::umu::Selection};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerDraft {
    pub umu: String,
    pub proton: String,
    pub runtime: String,
    pub exact: bool,
}
impl RunnerDraft {
    pub fn from_selection(selection: Option<Selection>) -> Self {
        let Some(selection) = selection else {
            return Self {
                umu: String::new(),
                proton: String::new(),
                runtime: String::new(),
                exact: false,
            };
        };
        let exact = [&selection.executable, &selection.proton, &selection.runtime]
            .iter()
            .any(|p| p.needs_byte_editor());
        let label = |p: &OsBytes| if exact { p.escaped() } else { p.label() };
        Self {
            umu: label(&selection.executable),
            proton: label(&selection.proton),
            runtime: label(&selection.runtime),
            exact,
        }
    }
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() > 2 * 1024 * 1024 {
            return Err(Error::Invalid(
                "The runner settings draft is too large.".into(),
            ));
        }
        serde_json::from_str(text)
            .map_err(|_| Error::Invalid("The runner settings draft is invalid.".into()))
    }
    pub fn selection(&self) -> Result<Selection> {
        Ok(Selection {
            executable: OsBytes::from_editor(&self.umu, self.exact)?,
            proton: OsBytes::from_editor(&self.proton, self.exact)?,
            runtime: OsBytes::from_editor(&self.runtime, self.exact)?,
        })
    }
    pub fn converted(&self, exact: bool) -> Result<Self> {
        let value = self.selection()?;
        if !exact
            && [&value.executable, &value.proton, &value.runtime]
                .iter()
                .any(|p| p.needs_byte_editor())
        {
            return Err(Error::Invalid(
                "These paths need exact Unix bytes mode to remain unchanged.".into(),
            ));
        }
        let label = |p: &OsBytes| if exact { p.escaped() } else { p.label() };
        Ok(Self {
            umu: label(&value.executable),
            proton: label(&value.proton),
            runtime: label(&value.runtime),
            exact,
        })
    }
    pub fn json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| Error::Invalid(e.to_string()))
    }
}
#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fixture assertions")]
mod tests {
    use super::*;
    #[test]
    fn exact_runner_paths_survive_editor_conversion_without_loss() {
        let draft = RunnerDraft::from_selection(Some(Selection {
            executable: OsBytes::new(b"/tool_\xff").unwrap(),
            proton: OsBytes::new(b"/p\\q").unwrap(),
            runtime: OsBytes::new(b"/rt").unwrap(),
        }));
        assert!(draft.exact);
        assert!(draft.converted(false).is_err());
        let selection = RunnerDraft::parse(&draft.json().unwrap())
            .unwrap()
            .selection()
            .unwrap();
        assert_eq!(selection.executable.as_bytes(), b"/tool_\xff");
        assert_eq!(selection.proton.as_bytes(), b"/p\\q");
    }
}
