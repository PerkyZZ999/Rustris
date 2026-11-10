use crate::{
    Error, Result,
    error::io,
    paths::{private_directory, verify_private_directory},
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub(crate) fn read(directory: &Path, name: &'static str, limit: u64) -> Result<Option<String>> {
    match fs::symlink_metadata(directory) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io("inspect the configuration directory", e)),
        Ok(_) => verify_private_directory(directory)?,
    }
    let path = directory.join(name);
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(&path)
    {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io("open the private configuration", e)),
        Ok(file) => file,
    };
    let metadata = file
        .metadata()
        .map_err(|e| io("inspect the private configuration", e))?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
        || metadata.len() > limit
    {
        return Err(Error::Invalid(format!(
            "{name} must be an owned private regular file within its size limit."
        )));
    }
    let mut text = String::new();
    file.take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|e| io("read the private configuration", e))?;
    if text.len() as u64 > limit {
        return Err(Error::Invalid(format!(
            "{name} exceeds its configuration size limit."
        )));
    }
    Ok(Some(text))
}

/// Callers first parse any existing version so unsupported data cannot be overwritten.
pub(crate) fn replace(directory: &Path, name: &'static str, text: &str) -> Result<()> {
    private_directory(directory)?;
    let temporary = directory.join(format!(".{name}-{}.tmp", uuid::Uuid::now_v7()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|e| io("prepare private configuration", e))?;
    let result = (|| {
        file.write_all(text.as_bytes())
            .map_err(|e| io("write private configuration", e))?;
        file.sync_all()
            .map_err(|e| io("sync private configuration", e))?;
        fs::rename(&temporary, directory.join(name))
            .map_err(|e| io("replace private configuration", e))?;
        File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|e| io("sync the configuration directory", e))
    })();
    if result.is_err() {
        match fs::remove_file(&temporary) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io("clean up a failed configuration write", e)),
        }
    }
    result
}
