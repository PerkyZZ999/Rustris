use rustris_core::{
    Error, Result,
    bytes::OsBytes,
    domain::{Entry, EnvDelta, ProfileInput, Runner},
};
use serde::{Deserialize, Serialize};
use std::os::unix::ffi::OsStrExt;
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentField {
    pub key: String,
    pub value: Option<String>,
}

/// Text presentation DTO; explicit mode controls conversion to exact execution bytes.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub id: Option<Uuid>,
    pub revision: i64,
    pub name: String,
    pub executable: String,
    pub cwd: String,
    pub arguments: Vec<String>,
    pub environment: Vec<EnvironmentField>,
    pub runner: String,
    pub prefix: String,
    pub exact: bool,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            id: None,
            revision: 0,
            name: String::new(),
            executable: String::new(),
            cwd: String::new(),
            arguments: Vec::new(),
            environment: Vec::new(),
            runner: "native".into(),
            prefix: String::new(),
            exact: false,
        }
    }
}

impl Draft {
    pub fn from_entry(entry: &Entry) -> Self {
        let profile = &entry.profile;
        let exact = std::iter::once(&profile.executable)
            .chain(std::iter::once(&profile.cwd))
            .chain(profile.arguments.iter())
            .chain(profile.prefix.iter())
            .chain(
                profile
                    .environment
                    .iter()
                    .flat_map(|e| std::iter::once(&e.key).chain(e.value.iter())),
            )
            .any(OsBytes::needs_byte_editor);
        let format = |value: &OsBytes| {
            if exact {
                value.escaped()
            } else {
                value.label()
            }
        };
        Self {
            id: Some(entry.game_id),
            revision: entry.revision,
            name: profile.name.clone(),
            executable: format(&profile.executable),
            cwd: format(&profile.cwd),
            arguments: profile.arguments.iter().map(&format).collect(),
            environment: profile
                .environment
                .iter()
                .map(|e| EnvironmentField {
                    key: format(&e.key),
                    value: e.value.as_ref().map(&format),
                })
                .collect(),
            runner: profile.runner.key().into(),
            prefix: profile.prefix.as_ref().map_or_else(String::new, format),
            exact,
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        if value.len() > 2 * 1024 * 1024 {
            return Err(Error::Invalid("The profile draft is too large.".into()));
        }
        serde_json::from_str(value).map_err(|_| {
            Error::Invalid("The profile draft is invalid. Reopen it and try again.".into())
        })
    }

    pub fn json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(|e| Error::Invalid(e.to_string()))
    }

    pub fn input(&self) -> Result<ProfileInput> {
        let decode = |text: &str| OsBytes::from_editor(text, self.exact);
        let executable = decode(&self.executable)?;
        let cwd = if self.cwd.is_empty() {
            let path = executable.path();
            let parent = path
                .parent()
                .ok_or_else(|| Error::Invalid("Choose an absolute executable path.".into()))?;
            OsBytes::new(parent.as_os_str().as_bytes())?
        } else {
            decode(&self.cwd)?
        };
        let runner = Runner::parse(&self.runner)?;
        let input = ProfileInput {
            name: self.name.clone(),
            executable,
            cwd,
            runner,
            prefix: if runner == Runner::Umu && !self.prefix.is_empty() {
                Some(decode(&self.prefix)?)
            } else {
                None
            },
            arguments: self
                .arguments
                .iter()
                .map(|value| decode(value))
                .collect::<Result<_>>()?,
            environment: self
                .environment
                .iter()
                .map(|e| {
                    Ok(EnvDelta {
                        key: decode(&e.key)?,
                        value: e.value.as_ref().map(|value| decode(value)).transpose()?,
                    })
                })
                .collect::<Result<_>>()?,
        };
        input.validate()?;
        Ok(input)
    }

    pub fn converted(mut self, exact: bool) -> Result<Self> {
        let convert = |text: &str| {
            let value = OsBytes::from_editor(text, self.exact)?;
            if !exact && value.needs_byte_editor() {
                return Err(Error::Invalid("This value needs exact byte mode. Replace the non-text bytes before switching.".into()));
            }
            Ok(if exact {
                value.escaped()
            } else {
                value.label()
            })
        };
        self.executable = convert(&self.executable)?;
        self.cwd = convert(&self.cwd)?;
        self.prefix = convert(&self.prefix)?;
        self.arguments = self
            .arguments
            .iter()
            .map(|value| convert(value))
            .collect::<Result<_>>()?;
        self.environment = self
            .environment
            .iter()
            .map(|e| {
                Ok(EnvironmentField {
                    key: convert(&e.key)?,
                    value: e.value.as_ref().map(|v| convert(v)).transpose()?,
                })
            })
            .collect::<Result<_>>()?;
        self.exact = exact;
        Ok(self)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Explicit fixture assertions")]
mod tests {
    use super::*;
    #[test]
    fn profile_json_editor_round_trip_preserves_every_non_nul_byte() {
        let mut path = b"/fixture/".to_vec();
        path.extend(1..=255);
        let profile = ProfileInput {
            name: "Fixture 🦀".into(),
            executable: OsBytes::new(path).unwrap(),
            cwd: OsBytes::new(b"/fixture").unwrap(),
            runner: Runner::Umu,
            prefix: Some(OsBytes::new(b"/prefix_\xfe").unwrap()),
            arguments: vec![
                OsBytes::new([]).unwrap(),
                OsBytes::new(b"a b;$(never)\\\xff").unwrap(),
            ],
            environment: vec![
                EnvDelta {
                    key: OsBytes::new(b"KEY_\xff").unwrap(),
                    value: None,
                },
                EnvDelta {
                    key: OsBytes::new(b"EMPTY").unwrap(),
                    value: Some(OsBytes::new([]).unwrap()),
                },
            ],
        };
        let entry = Entry {
            game_id: Uuid::now_v7(),
            installation_id: Uuid::now_v7(),
            profile_id: Uuid::now_v7(),
            revision: 7,
            favorite: true,
            artwork: None,
            artwork_available: false,
            review_approved: false,
            fetched_cover: None,
            fetched_hero: None,
            profile: profile.clone(),
            availability: String::new(),
        };
        let draft = Draft::from_entry(&entry);
        assert!(draft.exact);
        let restored = Draft::parse(&draft.json().unwrap()).unwrap();
        assert_eq!(restored.id, Some(entry.game_id));
        assert_eq!(restored.revision, 7);
        assert_eq!(restored.input().unwrap(), profile);
    }
    #[test]
    fn blank_draft_can_switch_byte_mode_without_inventing_paths() {
        let draft = Draft::default().converted(true).unwrap();
        assert!(draft.exact);
        assert!(draft.executable.is_empty());
    }
    #[test]
    fn mode_conversion_is_lossless_and_rejects_nontext_downgrade() {
        let mut draft = Draft {
            executable: "/Games/🦀\\game".into(),
            ..Draft::default()
        };
        draft = draft.converted(true).unwrap().converted(false).unwrap();
        assert_eq!(draft.executable, "/Games/🦀\\game");
        draft.exact = true;
        draft.executable = "/Games/\\xff".into();
        assert!(draft.converted(false).is_err());
    }
}
