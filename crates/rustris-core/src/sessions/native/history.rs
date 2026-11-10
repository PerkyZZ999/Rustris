use super::{
    LogRecord,
    capture::CHUNK,
    sink::{DISK_LIMIT, HEADER, MAGIC},
};
use crate::{Result, error::io, paths::verify_private_directory};
use rustix::fs::{FlockOperation, flock};
use std::{
    fs::{File, OpenOptions},
    io::{ErrorKind, Read},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
use uuid::Uuid;

#[derive(Debug)]
pub struct CompletedLog {
    pub records: Vec<LogRecord>,
    _file: File,
}

/// A bounded completed-log read holds a shared lock against retention eviction.
/// Active writers keep an exclusive lock, so incomplete live files are never read.
pub fn read_completed(directory: &Path, session: Uuid) -> Result<CompletedLog> {
    verify_private_directory(directory)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(directory.join(format!("{session}.log")))
        .map_err(|e| io("open the completed session log", e))?;
    let metadata = file
        .metadata()
        .map_err(|e| io("inspect the completed log", e))?;
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o077 != 0
        || metadata.len() > DISK_LIMIT
    {
        return Err(crate::Error::Invalid(
            "The completed log is not an owned private bounded file.".into(),
        ));
    }
    flock(&file, FlockOperation::NonBlockingLockShared)
        .map_err(|e| io("lock the completed log for reading", e.into()))?;
    let length = file
        .metadata()
        .map_err(|e| io("inspect the locked log", e))?
        .len();
    if length > DISK_LIMIT {
        return Err(crate::Error::Invalid(
            "The completed log exceeds the size limit.".into(),
        ));
    }
    let records = read_snapshot(&mut file, length)?;
    Ok(CompletedLog {
        records,
        _file: file,
    })
}

fn read_snapshot(file: &mut File, length: u64) -> Result<Vec<LogRecord>> {
    if length == 0 {
        return Ok(Vec::new());
    }
    let mut reader = file.take(length);
    let mut magic = [0; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|e| io("read the log version", e))?;
    if &magic != MAGIC {
        return Err(crate::Error::Invalid(
            "The session log version is unsupported.".into(),
        ));
    }
    let mut retained = std::collections::VecDeque::new();
    let mut bytes = 0;
    let mut sequence = 0;
    loop {
        let mut header = [0; HEADER];
        match reader.read(&mut header[..1]) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => return Err(io("read a log frame", e)),
        }
        reader
            .read_exact(&mut header[1..])
            .map_err(|e| io("read a complete log frame", e))?;
        let next = field(&header, 1);
        let length = u32::from_be_bytes([header[25], header[26], header[27], header[28]]) as usize;
        if ![1, 2].contains(&header[0]) || next <= sequence || length > CHUNK {
            return Err(io(
                "decode the session log",
                std::io::Error::new(
                    ErrorKind::InvalidData,
                    "Invalid stream, sequence or frame length",
                ),
            ));
        }
        sequence = next;
        let mut data = vec![0; length];
        reader
            .read_exact(&mut data)
            .map_err(|e| io("read the full log payload", e))?;
        bytes += length;
        retained.push_back(LogRecord {
            sequence: next,
            stream: header[0],
            timestamp_us: field(&header, 9),
            offset: field(&header, 17),
            data,
        });
        while bytes > 128 * 1024 || retained.len() > 256 {
            if let Some(record) = retained.pop_front() {
                bytes -= record.data.len();
            }
        }
    }
    if reader.limit() != 0 {
        return Err(io(
            "read the completed log snapshot",
            std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "The completed log was truncated while reading",
            ),
        ));
    }
    Ok(retained.into())
}

fn field(header: &[u8; HEADER], offset: usize) -> u64 {
    let mut value = [0; 8];
    value.copy_from_slice(&header[offset..offset + 8]);
    u64::from_be_bytes(value)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Isolated log snapshot fixture")]
mod tests {
    use super::*;
    use std::{
        fs,
        io::{Seek, SeekFrom, Write},
        os::unix::fs::DirBuilderExt,
    };

    fn frame(file: &mut File, sequence: u64, data: &[u8]) {
        file.write_all(&[1]).unwrap();
        file.write_all(&sequence.to_be_bytes()).unwrap();
        file.write_all(&1_u64.to_be_bytes()).unwrap();
        file.write_all(&0_u64.to_be_bytes()).unwrap();
        file.write_all(&(data.len() as u32).to_be_bytes()).unwrap();
        file.write_all(data).unwrap();
    }

    #[test]
    fn snapshot_read_ignores_later_growth_and_rejects_truncation() {
        let root = std::env::temp_dir().join(format!("rustris-log-snapshot-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(root.join("log"))
            .unwrap();
        file.write_all(MAGIC).unwrap();
        frame(&mut file, 1, b"first");
        let length = file.stream_position().unwrap();
        frame(&mut file, 2, b"later");
        file.seek(SeekFrom::Start(0)).unwrap();
        let records = read_snapshot(&mut file, length).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].sequence, 1);
        assert_eq!(records[0].data, b"first");
        assert_eq!(file.stream_position().unwrap(), length);
        file.set_len(MAGIC.len() as u64).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        assert!(read_snapshot(&mut file, length).is_err());
        drop(file);
        fs::remove_dir_all(root).unwrap();
    }
}
