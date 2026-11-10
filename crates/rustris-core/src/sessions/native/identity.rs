use crate::{Error, Result, bytes::OsBytes, paths::private_directory, sessions::HelperIdentity};
use std::{
    fs,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};
use uuid::Uuid;

/// Reconnection evidence only. This function does not open signaling authority.
pub fn observe_process(pid: u32, socket: OsBytes) -> Result<HelperIdentity> {
    if pid == 0 {
        return Err(Error::Invalid(
            "A process identity cannot use PID zero.".into(),
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(format!("/proc/{pid}/stat"))
        .and_then(|file| file.take(4097).read_to_end(&mut bytes))
        .map_err(|e| crate::error::io("observe the helper identity", e))?;
    if bytes.len() > 4096 {
        return Err(Error::Invalid(
            "The helper process record exceeds its limit.".into(),
        ));
    }
    let close = bytes
        .iter()
        .rposition(|&byte| byte == b')')
        .ok_or_else(|| Error::Invalid("The helper process record is malformed.".into()))?;
    let state = bytes[close + 1..]
        .split(|byte| byte.is_ascii_whitespace())
        .find(|part| !part.is_empty());
    if matches!(state, Some(b"Z" | b"X")) {
        return Err(Error::Invalid(
            "The original helper process has exited.".into(),
        ));
    }
    let ticks = bytes[close + 1..]
        .split(|byte| byte.is_ascii_whitespace())
        .filter(|part| !part.is_empty())
        .nth(19)
        .ok_or_else(|| Error::Invalid("The helper start time is missing.".into()))?;
    let start_ticks = std::str::from_utf8(ticks)
        .ok()
        .and_then(|text| text.parse::<u64>().ok())
        .ok_or_else(|| Error::Invalid("The helper start time is malformed.".into()))?;
    let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .map_err(|e| crate::error::io("observe the boot identity", e))?;
    let boot_id = Uuid::parse_str(boot.trim())
        .map_err(|_| Error::Invalid("The boot identity is malformed.".into()))?;
    Ok(HelperIdentity {
        pid,
        start_ticks,
        boot_id,
        socket,
    })
}

/// Prefer an owned XDG runtime directory; use a short private Linux path if needed.
/// Socket path length is independent of library/checkout paths.
pub fn private_runtime(preferred: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = preferred.filter(|path| {
        path.is_absolute()
            && path
                .join("rustris")
                .join(format!("{}.sock", Uuid::nil()))
                .as_os_str()
                .as_bytes()
                .len()
                < 108
    }) {
        private_directory(path)?;
        let directory = path.join("rustris");
        private_directory(&directory)?;
        return Ok(directory);
    }
    let path = PathBuf::from(format!(
        "/tmp/rustris-{}",
        rustix::process::geteuid().as_raw()
    ));
    private_directory(&path)?;
    Ok(path)
}
