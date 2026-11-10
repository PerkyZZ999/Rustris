use super::capture::LogRecord;
use crate::{
    paths::{Paths, private_directory},
    store::Store,
};
use rusqlite::OptionalExtension;
use rustix::fs::{FlockOperation, flock};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub(super) const DISK_LIMIT: u64 = 16 * 1024 * 1024;
const GLOBAL_LIMIT: u64 = 256 * 1024 * 1024;
struct Candidate {
    completed: i64,
    path: PathBuf,
    bytes: u64,
    device: u64,
    inode: u64,
}
struct Inventory {
    bytes: u64,
    candidates: Vec<Candidate>,
}
pub(super) const HEADER: usize = 29;
pub(super) const MAGIC: &[u8; 8] = b"RSTRLOG\x01";
pub(super) struct Written {
    pub total: u64,
    pub payload: u64,
    pub evicted: u64,
}
pub(super) struct Sink {
    file: File,
    budget: File,
    directory: PathBuf,
    session: Uuid,
    store: Store,
    bytes: u64,
    disk_limit: u64,
    global_limit: u64,
}
struct BudgetLock<'a>(&'a File);
impl Drop for BudgetLock<'_> {
    fn drop(&mut self) {
        if let Err(error) = flock(self.0, FlockOperation::Unlock) {
            tracing::error!(%error,"Log budget unlock failed");
        }
    }
}
impl Sink {
    pub fn open(paths: &Paths, directory: &Path, session: Uuid) -> io::Result<Self> {
        private_directory(directory).map_err(io::Error::other)?;
        let budget = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(
                (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
            )
            .open(directory.join(".budget.lock"))?;
        let budget_meta = budget.metadata()?;
        if !budget_meta.is_file()
            || budget_meta.uid() != rustix::process::geteuid().as_raw()
            || budget_meta.mode() & 0o077 != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Log budget lock is not an owned private regular file",
            ));
        }
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(directory.join(format!("{session}.log")))?;
        flock(&file, FlockOperation::LockExclusive)?;
        let store = Store::open_existing(paths.clone()).map_err(io::Error::other)?;
        Ok(Self {
            file,
            budget,
            directory: directory.into(),
            session,
            store,
            bytes: 0,
            disk_limit: DISK_LIMIT,
            global_limit: GLOBAL_LIMIT,
        })
    }
    pub fn append(&mut self, record: &LogRecord) -> io::Result<Written> {
        if self.bytes + HEADER as u64 >= self.disk_limit {
            return Ok(Written {
                total: self.bytes,
                payload: 0,
                evicted: 0,
            });
        }
        flock(&self.budget, FlockOperation::LockExclusive)?;
        let _lock = BudgetLock(&self.budget);
        let Inventory {
            bytes: mut total,
            mut candidates,
        } = self.files()?;
        candidates.sort_by_key(|candidate| candidate.completed);
        let prefix = if self.bytes == 0 {
            MAGIC.len() as u64
        } else {
            0
        };
        let requested = record.data.len() as u64 + HEADER as u64 + prefix;
        let mut evicted = 0;
        for candidate in candidates {
            if total.saturating_add(requested) <= self.global_limit {
                break;
            }
            match fs::symlink_metadata(candidate.path.with_extension("pin")) {
                Ok(_) => continue,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
            let file = open_candidate(&candidate)?;
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => {}
                Err(rustix::io::Errno::AGAIN) => continue,
                Err(error) => return Err(error.into()),
            }
            let current = fs::symlink_metadata(&candidate.path)?;
            if !same_candidate(&candidate, &current) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Completed log changed during retention",
                ));
            }
            fs::remove_file(&candidate.path)?;
            total = total.saturating_sub(candidate.bytes);
            evicted += 1;
        }
        let room = (self.disk_limit - self.bytes).min(self.global_limit.saturating_sub(total));
        let payload = room
            .saturating_sub(HEADER as u64 + prefix)
            .min(record.data.len() as u64) as usize;
        if payload == 0 {
            return Ok(Written {
                total: self.bytes,
                payload: 0,
                evicted,
            });
        }
        let mut header = [0; HEADER];
        header[0] = record.stream;
        header[1..9].copy_from_slice(&record.sequence.to_be_bytes());
        header[9..17].copy_from_slice(&record.timestamp_us.to_be_bytes());
        header[17..25].copy_from_slice(&record.offset.to_be_bytes());
        header[25..29].copy_from_slice(&(payload as u32).to_be_bytes());
        if prefix > 0 {
            self.file.write_all(MAGIC)?;
        }
        self.file.write_all(&header)?;
        self.file.write_all(&record.data[..payload])?;
        self.bytes += HEADER as u64 + payload as u64 + prefix;
        Ok(Written {
            total: self.bytes,
            payload: payload as u64,
            evicted,
        })
    }
    fn files(&self) -> io::Result<Inventory> {
        let mut bytes = 0_u64;
        let mut candidates = Vec::new();
        let mut count = 0;
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            count += 1;
            if count > 10000 {
                return Err(io::Error::other(
                    "Log directory exceeds its bounded scan limit",
                ));
            }
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "log") {
                continue;
            }
            let meta = fs::symlink_metadata(&path)?;
            if !meta.is_file() {
                continue;
            }
            bytes = bytes.saturating_add(meta.len());
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Ok(id) = Uuid::parse_str(stem) else {
                continue;
            };
            if id == self.session
                || meta.uid() != rustix::process::geteuid().as_raw()
                || meta.mode() & 0o077 != 0
            {
                continue;
            }
            let completed:Option<i64>=self.store.connection.query_row("SELECT completed_ms FROM launch_sessions WHERE id=?1 AND state IN ('exited','stopped','cancelled','launch_failed')",[id.as_bytes().as_slice()],|row|row.get(0)).optional().map_err(io::Error::other)?.flatten();
            if let Some(completed) = completed {
                candidates.push(Candidate {
                    completed,
                    path,
                    bytes: meta.len(),
                    device: meta.dev(),
                    inode: meta.ino(),
                });
            }
        }
        Ok(Inventory { bytes, candidates })
    }
    pub fn physical_size(&self) -> io::Result<u64> {
        Ok(self.file.metadata()?.len())
    }
    pub fn flush(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.sync_all()
    }
}

fn same_candidate(candidate: &Candidate, metadata: &fs::Metadata) -> bool {
    metadata.is_file()
        && metadata.dev() == candidate.device
        && metadata.ino() == candidate.inode
        && metadata.len() == candidate.bytes
        && metadata.uid() == rustix::process::geteuid().as_raw()
        && metadata.mode() & 0o077 == 0
}

fn open_candidate(candidate: &Candidate) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(&candidate.path)?;
    if !same_candidate(candidate, &file.metadata()?) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Completed log changed during retention",
        ));
    }
    Ok(file)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Explicit isolated retention assertions")]
mod tests {
    use super::*;
    use crate::{
        bytes::OsBytes,
        domain::{ProfileInput, Runner},
        sessions::{BaseEnvironment, SessionState},
    };
    struct Fixture {
        root: PathBuf,
        paths: Paths,
        directory: PathBuf,
        store: Store,
    }
    impl Fixture {
        fn new() -> Self {
            let root = PathBuf::from(format!("/tmp/rs-retain-{}", Uuid::new_v4().simple()));
            private_directory(&root).unwrap();
            let paths = Paths::in_root(&root.join("library"));
            let store = Store::open(paths.clone()).unwrap();
            let directory = root.join("logs");
            private_directory(&directory).unwrap();
            Self {
                root,
                paths,
                directory,
                store,
            }
        }
        fn session(&mut self, completed: bool) -> Uuid {
            let input = ProfileInput {
                name: "Retention fixture".into(),
                executable: OsBytes::new(b"/missing").unwrap(),
                cwd: OsBytes::new(b"/").unwrap(),
                arguments: vec![],
                environment: vec![],
                runner: Runner::Native,
                prefix: None,
            };
            let game = self.store.save(None, &input).unwrap();
            let (session, owner) = self
                .store
                .reserve_session(game, 1, &BaseEnvironment::default())
                .unwrap();
            if completed {
                self.store
                    .transition_session(
                        &owner,
                        0,
                        SessionState::Cancelled,
                        "No process was spawned",
                        None,
                    )
                    .unwrap();
            }
            session.id
        }
        fn log(&self, id: Uuid, bytes: usize) -> PathBuf {
            let path = self.directory.join(format!("{id}.log"));
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)
                .unwrap()
                .write_all(&vec![b'x'; bytes])
                .unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn budget_lock_rejects_symlink_and_fifo() {
        for use_fifo in [false, true] {
            let mut fixture = Fixture::new();
            let current = fixture.session(false);
            let lock = fixture.directory.join(".budget.lock");
            let foreign = fixture.root.join("foreign");
            fs::write(&foreign, b"preserve me").unwrap();
            if use_fifo {
                rustix::fs::mknodat(
                    rustix::fs::CWD,
                    &lock,
                    rustix::fs::FileType::Fifo,
                    rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
                    0,
                )
                .unwrap();
            } else {
                std::os::unix::fs::symlink(&foreign, &lock).unwrap();
            }
            assert!(Sink::open(&fixture.paths, &fixture.directory, current).is_err());
            assert_eq!(fs::read(&foreign).unwrap(), b"preserve me");
            assert!(fs::symlink_metadata(&lock).is_ok());
        }
    }
    #[test]
    #[ignore = "Only invoked as a bounded child by the retention FIFO regression"]
    fn retention_fifo_child() {
        let candidate = Candidate {
            completed: 0,
            path: PathBuf::from(std::env::var_os("RUSTRIS_RETENTION_TEST_PATH").unwrap()),
            bytes: 1,
            device: std::env::var("RUSTRIS_RETENTION_TEST_DEVICE")
                .unwrap()
                .parse()
                .unwrap(),
            inode: std::env::var("RUSTRIS_RETENTION_TEST_INODE")
                .unwrap()
                .parse()
                .unwrap(),
        };
        assert!(open_candidate(&candidate).is_err());
    }

    #[test]
    fn replaced_retention_candidate_is_refused_without_blocking() {
        use std::time::{Duration, Instant};
        let mut fixture = Fixture::new();
        let previous = fixture.session(true);
        let path = fixture.log(previous, 1);
        let original = File::open(&path).unwrap();
        let metadata = original.metadata().unwrap();
        let candidate = Candidate {
            completed: 0,
            path: path.clone(),
            bytes: 1,
            device: metadata.dev(),
            inode: metadata.ino(),
        };
        assert!(open_candidate(&candidate).is_ok());
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"growth")
            .unwrap();
        assert!(open_candidate(&candidate).is_err());
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(1)
            .unwrap();
        assert!(open_candidate(&candidate).is_ok());
        fs::remove_file(&path).unwrap();
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &path,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
            0,
        )
        .unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "sessions::native::sink::tests::retention_fifo_child",
            ])
            .env("RUSTRIS_RETENTION_TEST_PATH", &path)
            .env("RUSTRIS_RETENTION_TEST_DEVICE", metadata.dev().to_string())
            .env("RUSTRIS_RETENTION_TEST_INODE", metadata.ino().to_string())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let outcome = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break Some(status);
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(
            outcome.is_some_and(|status| status.success()),
            "Retention FIFO open blocked or failed"
        );
        fs::remove_file(&path).unwrap();
        let foreign = fixture.root.join("foreign");
        fs::write(&foreign, b"preserve me").unwrap();
        std::os::unix::fs::symlink(&foreign, &path).unwrap();
        assert!(open_candidate(&candidate).is_err());
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"replacement").unwrap();
        assert!(open_candidate(&candidate).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        assert_eq!(fs::read(&foreign).unwrap(), b"preserve me");
    }
    #[test]
    fn retention_preserves_active_pinned_foreign_and_recovery_files() {
        let mut fixture = Fixture::new();
        let pinned = fixture.session(true);
        let pinned_path = fixture.log(pinned, 2048);
        File::create(pinned_path.with_extension("pin")).unwrap();
        let old = fixture.session(true);
        let old_path = fixture.log(old, 2048);
        let active = fixture.session(false);
        let active_path = fixture.log(active, 2048);
        fs::write(fixture.directory.join("foreign.log"), vec![b'f'; 2048]).unwrap();
        fs::write(fixture.directory.join("keep.sqlite3"), b"recovery marker").unwrap();
        let current = fixture.session(false);
        let mut sink = Sink::open(&fixture.paths, &fixture.directory, current).unwrap();
        sink.disk_limit = 4096;
        sink.global_limit = 8192;
        let first = sink
            .append(&LogRecord {
                sequence: 1,
                stream: 1,
                timestamp_us: 1,
                offset: 0,
                data: vec![b'a'; 512],
            })
            .unwrap();
        assert_eq!(first.evicted, 1);
        assert!(!old_path.exists());
        assert!(pinned_path.exists() && active_path.exists());
        let second = sink
            .append(&LogRecord {
                sequence: 2,
                stream: 2,
                timestamp_us: 2,
                offset: 0,
                data: vec![b'b'; 4096],
            })
            .unwrap();
        assert!(second.payload < 4096);
        assert_eq!(sink.files().unwrap().bytes, 8192);
        assert_eq!(
            fs::read(fixture.directory.join("keep.sqlite3")).unwrap(),
            b"recovery marker"
        );
        assert_eq!(
            fs::metadata(fixture.directory.join("foreign.log"))
                .unwrap()
                .len(),
            2048
        );
    }
    #[test]
    fn a_live_shared_reader_prevents_completed_log_eviction() {
        let mut fixture = Fixture::new();
        let previous = fixture.session(true);
        let path = fixture.log(previous, 4096);
        let reader = File::open(&path).unwrap();
        flock(&reader, FlockOperation::LockShared).unwrap();
        let current = fixture.session(false);
        let mut sink = Sink::open(&fixture.paths, &fixture.directory, current).unwrap();
        sink.disk_limit = 4096;
        sink.global_limit = 4096;
        let record = LogRecord {
            sequence: 1,
            stream: 1,
            timestamp_us: 1,
            offset: 0,
            data: vec![b'x'; 512],
        };
        assert_eq!(sink.append(&record).unwrap().payload, 0);
        assert!(path.exists());
        flock(&reader, FlockOperation::Unlock).unwrap();
        assert_eq!(sink.append(&record).unwrap().evicted, 1);
        assert!(!path.exists());
    }
    #[test]
    fn production_cap_evicts_only_completed_logs() {
        let mut fixture = Fixture::new();
        let older = fixture.session(true);
        let newer = fixture.session(true);
        fixture
            .store
            .connection
            .execute(
                "UPDATE launch_sessions SET completed_ms=?1 WHERE id=?2",
                rusqlite::params![1_i64, older.as_bytes().as_slice()],
            )
            .unwrap();
        fixture
            .store
            .connection
            .execute(
                "UPDATE launch_sessions SET completed_ms=?1 WHERE id=?2",
                rusqlite::params![2_i64, newer.as_bytes().as_slice()],
            )
            .unwrap();
        let older_log = fixture.directory.join(format!("{older}.log"));
        let newer_log = fixture.directory.join(format!("{newer}.log"));
        for path in [&older_log, &newer_log] {
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .unwrap()
                .set_len(200 * 1024 * 1024)
                .unwrap();
        }
        let backup = fixture.paths.data.join("schema-3-backup-older.sqlite3");
        fs::write(&backup, b"older-backup-not-a-library").unwrap();
        let library_before = fs::read(fixture.paths.database()).unwrap();
        let current = fixture.session(false);
        let mut sink = Sink::open(&fixture.paths, &fixture.directory, current).unwrap();
        assert_eq!(sink.disk_limit, 16 * 1024 * 1024);
        assert_eq!(sink.global_limit, 256 * 1024 * 1024);
        let written = sink
            .append(&LogRecord {
                sequence: 1,
                stream: 1,
                timestamp_us: 1,
                offset: 0,
                data: vec![b'a'; 64],
            })
            .unwrap();
        assert_eq!(written.evicted, 1);
        assert!(!older_log.exists());
        assert!(newer_log.exists());
        assert_eq!(fs::read(&backup).unwrap(), b"older-backup-not-a-library");
        assert_eq!(fs::read(fixture.paths.database()).unwrap(), library_before);
        let games: i64 = fixture
            .store
            .connection
            .query_row("SELECT count(*) FROM games", [], |row| row.get(0))
            .unwrap();
        let older_state: String = fixture
            .store
            .connection
            .query_row(
                "SELECT state FROM launch_sessions WHERE id=?1",
                [older.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .unwrap();
        assert!(games >= 3);
        assert_eq!(older_state, "cancelled");
    }
}
