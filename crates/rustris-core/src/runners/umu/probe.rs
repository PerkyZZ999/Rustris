use crate::{Error, Result, error::io};
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const RETAIN: usize = 4096;
const DRAIN_LIMIT: usize = 8192;
const DEADLINE: Duration = Duration::from_secs(2);

/// A short, directly owned version query, never a game or detached session.
pub(super) fn version(command: &mut Command) -> Result<String> {
    let mut child = command
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| io("query the selected UMU version", e))?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Invalid("The version output pipe is unavailable.".into()))?;
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Invalid("The version error pipe is unavailable.".into()))?;
        for fd in [
            &stdout as &dyn std::os::fd::AsFd,
            &stderr as &dyn std::os::fd::AsFd,
        ] {
            let flags = rustix::fs::fcntl_getfl(fd)
                .map_err(|e| io("inspect a version probe pipe", e.into()))?;
            rustix::fs::fcntl_setfl(fd, flags | rustix::fs::OFlags::NONBLOCK)
                .map_err(|e| io("bound a version probe pipe", e.into()))?;
        }
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let mut drained = 0;
        let deadline = Instant::now() + DEADLINE;
        loop {
            drain(&mut stdout, &mut output, &mut drained)?;
            drain(&mut stderr, &mut errors, &mut drained)?;
            if Instant::now() >= deadline || drained > DRAIN_LIMIT {
                return Err(Error::Invalid(
                    "The UMU version query exceeded its time/output budget. No game was started."
                        .into(),
                ));
            }
            if let Some(status) = child
                .try_wait()
                .map_err(|e| io("observe the version query", e))?
            {
                drain(&mut stdout, &mut output, &mut drained)?;
                drain(&mut stderr, &mut errors, &mut drained)?;
                if drained > DRAIN_LIMIT {
                    return Err(Error::Invalid(
                        "The UMU version output exceeds its bounded limit.".into(),
                    ));
                }
                if !status.success() {
                    return Err(Error::Invalid(
                        "The selected UMU version query failed. No game was started.".into(),
                    ));
                }
                return String::from_utf8(output).map_err(|_| {
                    Error::Invalid("The UMU version output is not valid UTF-8.".into())
                });
            }
            if Instant::now() >= deadline || drained > DRAIN_LIMIT {
                return Err(Error::Invalid(
                    "The UMU version query exceeded its time/output budget. No game was started."
                        .into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    if result.is_err()
        && child
            .try_wait()
            .map_err(|e| io("observe the failed version query", e))?
            .is_none()
    {
        // Child is still unreaped and directly owned; this is not saved-PID control.
        child
            .kill()
            .map_err(|e| io("end the bounded version query", e))?;
        child
            .wait()
            .map_err(|e| io("reap the bounded version query", e))?;
    }
    result
}
fn drain(reader: &mut impl Read, retained: &mut Vec<u8>, total: &mut usize) -> Result<()> {
    let mut chunk = [0; 4096];
    for _ in 0..16 {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                *total = total.saturating_add(count);
                let keep = count.min(RETAIN.saturating_sub(retained.len()));
                retained.extend_from_slice(&chunk[..keep]);
                if keep < count {
                    return Err(Error::Invalid(
                        "The UMU version output exceeds its bounded limit.".into(),
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(e) => return Err(io("drain the version query", e)),
        }
    }
    Ok(())
}
