use crate::{Result, sessions::LaunchSnapshot};
use serde::{Deserialize, Serialize};
use std::os::unix::fs::MetadataExt;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: Uuid,
    pub resource: String,
    pub source: String,
    pub observed_ms: i64,
    pub outcome: String,
    pub errno: Option<i32>,
    pub device: Option<u64>,
    pub inode: Option<u64>,
    pub size: Option<u64>,
    pub modified_seconds: Option<i64>,
    pub modified_nanoseconds: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub rule: String,
    pub evidence: Uuid,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Preflight {
    pub observations: Vec<Observation>,
    pub findings: Vec<Finding>,
}
impl Preflight {
    pub(crate) fn validate(&self) -> Result<()> {
        let mut ids = std::collections::BTreeSet::new();
        if self.observations.len() > 32 || self.findings.len() > 32 {
            return Err(crate::Error::Invalid(
                "Preflight evidence exceeds its record limit.".into(),
            ));
        }
        for observation in &self.observations {
            if observation.id.is_nil()
                || !ids.insert(observation.id)
                || observation.observed_ms <= 0
                || observation.resource.is_empty()
                || observation.resource.len() > 64
                || observation.source.is_empty()
                || observation.source.len() > 256
                || ![
                    "available",
                    "missing",
                    "inaccessible",
                    "wrong_type",
                    "unknown",
                    "unsupported",
                ]
                .contains(&observation.outcome.as_str())
            {
                return Err(crate::Error::Invalid(
                    "Preflight observation identity, source, time or outcome is invalid.".into(),
                ));
            }
        }
        for finding in &self.findings {
            if !ids.contains(&finding.evidence)
                || finding.rule.is_empty()
                || finding.rule.len() > 128
                || finding.message.len() > 2048
                || finding.message.contains('\0')
            {
                return Err(crate::Error::Invalid(
                    "Preflight findings require valid bounded evidence links.".into(),
                ));
            }
        }
        Ok(())
    }
    /// Local availability observations only; passing never establishes game health.
    pub fn inspect(snapshot: &LaunchSnapshot) -> Result<Self> {
        let mut report = Self {
            observations: Vec::new(),
            findings: Vec::new(),
        };
        for (resource, value, directory) in [
            ("executable", &snapshot.source.profile.executable, false),
            ("working_directory", &snapshot.source.profile.cwd, true),
        ] {
            let mut observation = Observation {
                id: Uuid::now_v7(),
                resource: resource.into(),
                source: "filesystem metadata and access(2)".into(),
                observed_ms: crate::sessions::now_ms()?,
                outcome: "unknown".into(),
                errno: None,
                device: None,
                inode: None,
                size: None,
                modified_seconds: None,
                modified_nanoseconds: None,
            };
            let result = std::fs::metadata(value.path()).and_then(|meta| {
                observation.device = Some(meta.dev());
                observation.inode = Some(meta.ino());
                observation.size = Some(meta.len());
                observation.modified_seconds = Some(meta.mtime());
                observation.modified_nanoseconds = Some(meta.mtime_nsec());
                if (directory && !meta.is_dir()) || (!directory && !meta.is_file()) {
                    observation.outcome = "wrong_type".into();
                    return Ok(false);
                }
                rustix::fs::access(value.path(), rustix::fs::Access::EXEC_OK)
                    .map_err(Into::into)
                    .map(|()| true)
            });
            match result {
                Ok(true) => observation.outcome = "available".into(),
                Ok(false) => {}
                Err(error) => {
                    observation.errno = error.raw_os_error();
                    observation.outcome = match error.kind() {
                        std::io::ErrorKind::NotFound => "missing",
                        std::io::ErrorKind::PermissionDenied => "inaccessible",
                        _ => "unknown",
                    }
                    .into();
                }
            }
            if observation.outcome != "available" {
                report.findings.push(Finding{rule:format!("native.{resource}.availability"),evidence:observation.id,message:format!("The {} is {}. Check the saved path and access permissions before launching.",resource.replace('_'," "),observation.outcome.replace('_'," "))});
            }
            report.observations.push(observation);
        }
        Ok(report)
    }
}
