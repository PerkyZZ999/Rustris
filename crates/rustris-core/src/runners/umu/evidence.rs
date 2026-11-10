use crate::{
    Error, Result,
    sessions::native::{Finding, Observation, Preflight},
};
use std::{fs, io::Read, path::Path};
use uuid::Uuid;
const MAX_MANIFEST: u64 = 64 * 1024;
#[derive(Clone, Copy)]
pub(super) enum Kind {
    File,
    Directory,
    Executable,
}
pub(super) fn observe(
    evidence: &mut Preflight,
    resource: &str,
    path: &Path,
    kind: Kind,
) -> Result<()> {
    check(
        evidence,
        resource,
        "filesystem metadata and access(2)",
        || {
            let metadata = fs::metadata(path)
                .map_err(|e| crate::error::io("inspect a runner prerequisite", e))?;
            if matches!(kind, Kind::Directory) && !metadata.is_dir()
                || !matches!(kind, Kind::Directory) && !metadata.is_file()
            {
                return Err(Error::Invalid(
                    "The prerequisite has the wrong file type.".into(),
                ));
            }
            let access = if matches!(kind, Kind::Executable | Kind::Directory) {
                rustix::fs::Access::EXEC_OK | rustix::fs::Access::READ_OK
            } else {
                rustix::fs::Access::READ_OK
            };
            rustix::fs::access(path, access)
                .map_err(|e| crate::error::io("access a runner prerequisite", e.into()))?;
            Ok(())
        },
    )
}
pub(super) fn check(
    evidence: &mut Preflight,
    resource: &str,
    source: &str,
    collect: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let id = Uuid::now_v7();
    let result = collect();
    let observed_ms = crate::sessions::now_ms()?;
    let (outcome, errno) = match &result {
        Ok(()) => ("available", None),
        Err(Error::Io { source, .. }) => (
            match source.kind() {
                std::io::ErrorKind::NotFound => "missing",
                std::io::ErrorKind::PermissionDenied => "inaccessible",
                _ => "unknown",
            },
            source.raw_os_error(),
        ),
        Err(_) => ("unsupported", None),
    };
    evidence.observations.push(Observation {
        id,
        resource: resource.into(),
        source: source.into(),
        observed_ms,
        outcome: outcome.into(),
        errno,
        device: None,
        inode: None,
        size: None,
        modified_seconds: None,
        modified_nanoseconds: None,
    });
    if let Err(error) = result {
        evidence.findings.push(Finding {
            rule: format!("umu.{resource}.preparation"),
            evidence: id,
            message: error.to_string(),
        });
    }
    Ok(())
}
pub(super) fn read_text(path: &Path) -> Result<String> {
    let mut text = String::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_MANIFEST + 1).read_to_string(&mut text))
        .map_err(|e| crate::error::io("read a bounded runner manifest", e))?;
    if text.len() as u64 > MAX_MANIFEST {
        return Err(Error::Invalid("A runner manifest exceeds 64 KiB.".into()));
    }
    Ok(text)
}
