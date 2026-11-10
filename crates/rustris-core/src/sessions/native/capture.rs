mod workers;
use crate::{paths::Paths, sessions::native::sink::Sink};
use rustix::{
    event::{EventfdFlags, eventfd},
    fd::OwnedFd,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{self, Read},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub(super) const CHUNK: usize = 32 * 1024;
const MEMORY: usize = 1024 * 1024;
const MEMORY_CHUNKS: usize = 1024;
const QUEUE: usize = 16;
type Pipe = Box<dyn Read + Send>;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogStats {
    pub capture_unavailable: bool,
    pub disk_size_unknown: bool,
    pub errors_omitted: u64,
    pub stdout_read: u64,
    pub stderr_read: u64,
    pub memory_bytes: usize,
    pub memory_peak: usize,
    pub memory_dropped: u64,
    pub disk_bytes: u64,
    pub disk_payload: u64,
    pub disk_queue_dropped: u64,
    pub disk_limit_dropped: u64,
    pub disk_error_dropped: u64,
    pub evicted_logs: u64,
    pub errors: Vec<String>,
}
impl LogStats {
    pub(super) fn error(&mut self, message: impl AsRef<str>) {
        let message: String = message.as_ref().chars().take(256).collect();
        if self.errors.len() < 8 && !self.errors.contains(&message) {
            self.errors.push(message);
        } else if !self.errors.contains(&message) {
            self.errors_omitted = self.errors_omitted.saturating_add(1);
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogRecord {
    pub sequence: u64,
    pub stream: u8,
    pub timestamp_us: u64,
    pub offset: u64,
    pub data: Vec<u8>,
}
struct Capture {
    stats: LogStats,
    ring: VecDeque<Arc<LogRecord>>,
    next: u64,
}
impl Capture {
    fn ingest(&mut self, stream: u8, data: Vec<u8>) -> Arc<LogRecord> {
        let counter = if stream == 1 {
            &mut self.stats.stdout_read
        } else {
            &mut self.stats.stderr_read
        };
        let offset = *counter;
        *counter = counter.saturating_add(data.len() as u64);
        let timestamp_us = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_micros().min(u128::from(u64::MAX)) as u64,
            Err(_) => {
                self.stats.error("Log wall-clock time is unavailable");
                0
            }
        };
        let record = Arc::new(LogRecord {
            sequence: self.next,
            stream,
            timestamp_us,
            offset,
            data,
        });
        self.next = self.next.saturating_add(1);
        while self.stats.memory_bytes + record.data.len() > MEMORY
            || self.ring.len() >= MEMORY_CHUNKS
        {
            if let Some(old) = self.ring.pop_front() {
                self.stats.memory_bytes -= old.data.len();
                self.stats.memory_dropped = self
                    .stats
                    .memory_dropped
                    .saturating_add(old.data.len() as u64);
            } else {
                break;
            }
        }
        self.stats.memory_bytes += record.data.len();
        self.stats.memory_peak = self.stats.memory_peak.max(self.stats.memory_bytes);
        self.ring.push_back(record.clone());
        record
    }
}
fn locked(capture: &Mutex<Capture>) -> io::Result<std::sync::MutexGuard<'_, Capture>> {
    capture
        .lock()
        .map_err(|_| io::Error::other("Log memory is unavailable after a worker failure"))
}
struct Done {
    counter: Arc<AtomicUsize>,
    wake: Arc<OwnedFd>,
}
impl Drop for Done {
    fn drop(&mut self) {
        self.counter.fetch_add(1, Ordering::Release);
        if let Err(error) = rustix::io::write(&self.wake, &1_u64.to_ne_bytes()) {
            tracing::error!(%error,"Could not notify log worker completion");
        }
    }
}

pub(super) struct Logs {
    capture: Arc<Mutex<Capture>>,
    done: Arc<AtomicUsize>,
    pub wake: Arc<OwnedFd>,
    inputs: Vec<SyncSender<Pipe>>,
    threads: Vec<JoinHandle<()>>,
}
impl Logs {
    /// Allocate workers before spawning; a thread-allocation error cannot strand game pipes.
    pub fn prepare(paths: Paths, directory: PathBuf, session: Uuid) -> io::Result<Self> {
        let capture = Arc::new(Mutex::new(Capture {
            stats: LogStats::default(),
            ring: VecDeque::new(),
            next: 1,
        }));
        let done = Arc::new(AtomicUsize::new(0));
        let wake = Arc::new(eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?);
        let (sender, receiver) = mpsc::sync_channel::<Arc<LogRecord>>(QUEUE);
        let mut threads = Vec::new();
        let mut inputs = Vec::new();
        for stream in [1, 2] {
            let (input, received) = mpsc::sync_channel::<Pipe>(1);
            inputs.push(input);
            let shared = capture.clone();
            let chunks = sender.clone();
            let completed = Done {
                counter: done.clone(),
                wake: wake.clone(),
            };
            threads.push(workers::reader(
                stream, received, chunks, shared, completed,
            )?);
        }
        drop(sender);
        let shared = capture.clone();
        let completed = Done {
            counter: done.clone(),
            wake: wake.clone(),
        };
        threads.push(workers::writer(
            paths, directory, session, receiver, shared, completed,
        )?);
        Ok(Self {
            capture,
            done,
            wake,
            inputs,
            threads,
        })
    }
    pub fn attach(
        &mut self,
        stdout: impl Read + Send + 'static,
        stderr: impl Read + Send + 'static,
    ) -> io::Result<()> {
        if self.inputs.len() != 2 {
            return Err(io::Error::other("Log pipes have already been assigned"));
        }
        let mut inputs = self.inputs.drain(..);
        inputs
            .next()
            .ok_or_else(|| io::Error::other("stdout worker missing"))?
            .send(Box::new(stdout))
            .map_err(|_| io::Error::other("stdout worker unavailable"))?;
        inputs
            .next()
            .ok_or_else(|| io::Error::other("stderr worker missing"))?
            .send(Box::new(stderr))
            .map_err(|_| io::Error::other("stderr worker unavailable"))?;
        Ok(())
    }
    pub fn cancel_unattached(&mut self) {
        self.inputs.clear();
    }
    pub fn finished(&self) -> bool {
        self.done.load(Ordering::Acquire) == 3
    }
    pub fn stats(&self) -> io::Result<LogStats> {
        Ok(locked(&self.capture)?.stats.clone())
    }
    pub fn records(&self, after: u64) -> io::Result<(Vec<LogRecord>, bool)> {
        let capture = locked(&self.capture)?;
        let gap = capture
            .ring
            .front()
            .is_some_and(|record| after.saturating_add(1) < record.sequence);
        let mut bytes = 0;
        let mut records = Vec::new();
        for record in capture
            .ring
            .iter()
            .filter(|record| record.sequence > after)
            .take(64)
        {
            if bytes + record.data.len() > CHUNK {
                break;
            }
            bytes += record.data.len();
            records.push((**record).clone());
        }
        Ok((records, gap))
    }
    pub fn join(&mut self) -> io::Result<()> {
        self.inputs.clear();
        for thread in self.threads.drain(..) {
            thread
                .join()
                .map_err(|_| io::Error::other("A log worker failed"))?;
        }
        Ok(())
    }
}
