mod control;
use super::{
    capture::Logs,
    identity::observe_process,
    server::Server,
    status::{CompletionEvidence, NativeStatus},
};
use crate::{
    Error, Result,
    bytes::OsBytes,
    paths::{Paths, private_directory},
    sessions::{Session, SessionOwner, SessionState},
    store::Store,
};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    process::{self, Pid, PidfdFlags, Signal, WaitId, WaitIdOptions},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    os::{
        fd::{AsFd, OwnedFd},
        unix::ffi::OsStrExt,
    },
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use uuid::Uuid;

const TICK: Duration = Duration::from_millis(100);
const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CHILDREN: usize = 128;
struct Child {
    descriptor: OwnedFd,
    sent: Option<Signal>,
}
pub(super) struct Supervisor {
    store: Store,
    record: Session,
    owner: SessionOwner,
    server: Server,
    state: SessionState,
    children: BTreeMap<u32, Child>,
    root: Option<u32>,
    root_code: Option<i32>,
    root_signal: Option<i32>,
    spawned: bool,
    detached: bool,
    unknown: bool,
    unknown_reported: bool,
    error: Option<String>,
    signal: Option<Signal>,
    logs: Option<Logs>,
    log_directory: PathBuf,
    started: Instant,
}
fn pid(raw: u32) -> io::Result<Pid> {
    Pid::from_raw(i32::try_from(raw).map_err(io::Error::other)?)
        .ok_or_else(|| io::Error::other("PID zero is invalid"))
}
fn waiting(descriptor: &OwnedFd, keep: bool) -> io::Result<Option<process::WaitIdStatus>> {
    let mut options = WaitIdOptions::EXITED | WaitIdOptions::NOHANG;
    if keep {
        options |= WaitIdOptions::NOWAIT;
    }
    process::waitid(WaitId::PidFd(descriptor.as_fd()), options).map_err(Into::into)
}
impl Supervisor {
    pub fn open(
        paths: Paths,
        runtime: PathBuf,
        state_root: PathBuf,
        session: Uuid,
    ) -> Result<Self> {
        process::setsid().map_err(|e| crate::error::io("detach the session helper", e.into()))?;
        process::set_child_subreaper(Pid::from_raw(1))
            .map_err(|e| crate::error::io("establish native child ownership", e.into()))?;
        private_directory(&runtime)?;
        private_directory(&state_root)?;
        let log_directory = state_root.join("logs");
        private_directory(&log_directory)?;
        let mut store = Store::open_existing(paths)?;
        let record = store.session(session)?;
        if record.state != SessionState::Preparing
            || (record.snapshot.source.profile.runner == crate::domain::Runner::Umu
                && record.snapshot.runtime.is_none())
        {
            return Err(Error::Invalid(
                "Only a prepared session with required runner provenance can start this helper."
                    .into(),
            ));
        }
        let owner = store.session_capability(session)?;
        let path = runtime.join(format!("{session}.sock"));
        let socket = OsBytes::new(path.as_os_str().as_bytes())?;
        let server = Server::bind(&path, owner.clone())
            .map_err(|e| crate::error::io("bind the owned session socket", e))?;
        let identity = observe_process(std::process::id(), socket)?;
        let record = store.register_helper(&owner, record.generation, &identity)?;
        Ok(Self {
            store,
            record,
            owner,
            server,
            state: SessionState::Preparing,
            children: BTreeMap::new(),
            root: None,
            root_code: None,
            root_signal: None,
            spawned: false,
            detached: false,
            unknown: false,
            unknown_reported: false,
            error: None,
            signal: None,
            logs: None,
            log_directory,
            started: Instant::now(),
        })
    }
    fn status(&self, after: Option<u64>) -> NativeStatus {
        let mut status = NativeStatus {
            state: self.state.key().into(),
            durable_state: self.record.state.key().into(),
            generation: self.record.generation,
            root_pid: self.root,
            owned_children: self.children.len(),
            root_exit_code: self.root_code,
            root_signal: self.root_signal,
            spawned: self.spawned,
            detached: self.detached,
            tracking_unknown: self.unknown,
            can_stop: self.spawned
                && !self.unknown
                && !self.detached
                && !self.children.is_empty()
                && self.signal.is_none(),
            can_force: self.spawned
                && !self.unknown
                && !self.detached
                && !self.children.is_empty()
                && self.signal == Some(Signal::TERM),
            error: self.error.clone(),
            log_coverage: super::LogCoverage::for_runner(
                self.record.snapshot.source.profile.runner,
            ),
            game_health: "unknown".into(),
            client_errors: self.server.errors(),
            ..NativeStatus::default()
        };
        if let Some(logs) = &self.logs {
            match logs.stats() {
                Ok(stats) => status.logs = stats,
                Err(error) => {
                    status.error = Some(error.to_string());
                    status.logs.capture_unavailable = true;
                    status
                        .logs
                        .error("Captured output is unavailable after a log worker failure");
                }
            }
            if let Some(after) = after {
                match logs.records(after) {
                    Ok((records, gap)) => {
                        status.records = records;
                        status.log_gap = gap;
                    }
                    Err(error) => status.error = Some(error.to_string()),
                }
            }
        }
        status
    }
    fn persist(&mut self, next: SessionState, detail: &str, code: Option<i32>) -> Result<()> {
        match self
            .store
            .transition_session(&self.owner, self.record.generation, next, detail, code)
        {
            Ok(record) => {
                self.record = record;
                self.state = next;
                Ok(())
            }
            Err(error) => {
                self.unknown = true;
                self.error = Some(error.to_string());
                Err(error)
            }
        }
    }
    fn start(&mut self) -> Result<()> {
        if self.state != SessionState::Preparing || self.spawned {
            return Err(Error::Invalid(
                "This session cannot spawn another process.".into(),
            ));
        }
        let resolved = crate::sessions::execution::resolve(&self.record.snapshot)?;
        self.record =
            self.store
                .record_preflight(&self.owner, self.record.generation, &resolved.report)?;
        let Some(mut command) = resolved.command else {
            return self.persist(
                SessionState::LaunchFailed,
                "Preparation blocked spawn; see linked preflight evidence.",
                None,
            );
        };
        let mut logs = Logs::prepare(
            self.store.paths.clone(),
            self.log_directory.clone(),
            self.record.id,
        )
        .map_err(|e| crate::error::io("prepare session output workers", e))?;
        crate::sessions::execution::verify_spawn_scope(&self.store, &self.record)?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(error) => {
                logs.cancel_unattached();
                logs.join()
                    .map_err(|e| crate::error::io("finish unstarted output workers", e))?;
                self.logs = Some(logs);
                return self.persist(
                    SessionState::LaunchFailed,
                    &format!("Native spawn failed: {error}"),
                    None,
                );
            }
        };
        self.spawned = true;
        self.root = Some(child.id());
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Invalid("The native stdout pipe is unavailable.".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| Error::Invalid("The native stderr pipe is unavailable.".into()))?;
        let attached = logs.attach(stdout, stderr);
        self.logs = Some(logs);
        attached.map_err(|e| crate::error::io("attach native output capture", e))?;
        let descriptor = process::pidfd_open(
            pid(child.id()).map_err(|e| crate::error::io("validate the native PID", e))?,
            PidfdFlags::empty(),
        )
        .map_err(|e| crate::error::io("open native ownership evidence", e.into()))?;
        let exited = waiting(&descriptor, true)
            .map_err(|e| crate::error::io("verify native child ownership", e))?
            .is_some();
        self.children.insert(
            child.id(),
            Child {
                descriptor,
                sent: None,
            },
        );
        if exited {
            self.observe()
                .map_err(|e| crate::error::io("observe the fast native exit", e))?;
            if self.children.is_empty() {
                return self.persist(
                    SessionState::Finalizing,
                    "The native process exited before a running observation; draining output.",
                    self.root_code,
                );
            }
        }
        self.persist(
            SessionState::Running,
            "Native process observed; game health remains unknown.",
            None,
        )
    }
    fn observe(&mut self) -> io::Result<()> {
        self.adopt()?;
        let mut exited = Vec::new();
        for (&raw, child) in &self.children {
            if let Some(status) = waiting(&child.descriptor, true)? {
                exited.push((raw, status.exit_status(), status.terminating_signal()));
            }
        }
        for (raw, code, signal) in exited {
            let child = self
                .children
                .remove(&raw)
                .ok_or_else(|| io::Error::other("Owned child disappeared"))?;
            waiting(&child.descriptor, false)?;
            if self.root == Some(raw) {
                self.root_code = code;
                self.root_signal = signal;
            }
        }
        self.adopt()?;
        if !self.unknown
            && let Some(signal) = self.signal
        {
            for child in self.children.values_mut() {
                if child.sent == Some(signal) {
                    continue;
                }
                if waiting(&child.descriptor, true)?.is_none() {
                    match process::pidfd_send_signal(&child.descriptor, signal) {
                        Ok(()) | Err(rustix::io::Errno::SRCH) => child.sent = Some(signal),
                        Err(error) => return Err(error.into()),
                    }
                }
            }
        }
        Ok(())
    }
    fn adopt(&mut self) -> io::Result<()> {
        let mut children = BTreeSet::new();
        for task in fs::read_dir("/proc/self/task")? {
            match fs::read_to_string(task?.path().join("children")) {
                Ok(text) => {
                    for word in text.split_whitespace() {
                        children.insert(word.parse::<u32>().map_err(io::Error::other)?);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        for raw in children {
            if self.children.contains_key(&raw) {
                continue;
            }
            if self.children.len() >= MAX_CHILDREN {
                return Err(io::Error::other(
                    "The verified child limit was exceeded; tracking is incomplete",
                ));
            }
            let descriptor = process::pidfd_open(pid(raw)?, PidfdFlags::empty())?;
            waiting(&descriptor, true)?;
            self.children.insert(
                raw,
                Child {
                    descriptor,
                    sent: None,
                },
            );
        }
        Ok(())
    }
    fn lose_tracking(&mut self, error: impl std::fmt::Display) {
        if self.unknown_reported {
            return;
        }
        self.unknown_reported = true;
        self.unknown = true;
        self.error = Some(error.to_string());
        self.state = SessionState::Unknown;
        match self.store.mark_session_unknown(
            self.record.id,
            self.record.generation,
            "Native ownership or persistence evidence was lost; no reservation was released.",
        ) {
            Ok(record) => self.record = record,
            Err(error) => {
                self.error = Some(format!("Tracking and storage are unavailable: {error}"))
            }
        }
    }
    fn wait(&self) -> io::Result<()> {
        let mut descriptors = self.server.descriptors();
        descriptors.extend(
            self.children
                .values()
                .map(|child| PollFd::new(&child.descriptor, PollFlags::IN)),
        );
        if let Some(logs) = &self.logs {
            descriptors.push(PollFd::new(logs.wake.as_ref(), PollFlags::IN));
        }
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: TICK.as_nanos() as i64,
        };
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(_) | Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
        if let Some(logs) = &self.logs {
            let mut bytes = [0; 8];
            match rustix::io::read(logs.wake.as_ref(), &mut bytes) {
                Ok(_) | Err(rustix::io::Errno::AGAIN) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn run(&mut self) -> Result<()> {
        loop {
            match self.server.collect() {
                Ok(requests) => {
                    for (id, operation) in requests {
                        let reply = self.handle(operation);
                        if let Err(error) = self.server.respond(id, reply) {
                            self.error = Some(error.to_string());
                        }
                    }
                }
                Err(error) => self.lose_tracking(error),
            }
            self.server.flush();
            if !self.spawned
                && self.state == SessionState::Preparing
                && self.started.elapsed() > PREPARATION_TIMEOUT
            {
                self.persist(SessionState::Cancelled,"Preparation expired without an authorized Start request; no process was spawned.",None)?;
            }
            if self.spawned {
                if let Err(error) = self.observe() {
                    self.lose_tracking(error);
                }
                if self.children.is_empty() && self.logs.as_ref().is_some_and(Logs::finished) {
                    if let Some(logs) = &mut self.logs
                        && let Err(error) = logs.join()
                    {
                        self.error = Some(error.to_string());
                    }
                    if !self.unknown {
                        if self.record.state != SessionState::Finalizing {
                            self.persist(
                                SessionState::Finalizing,
                                "Owned processes exited; final output capture completed.",
                                self.root_code,
                            )?;
                        }
                        let mut evidence = CompletionEvidence {
                            kind: if self.record.snapshot.runtime.is_some() {
                                "umu_completion_v1"
                            } else {
                                "native_completion_v1"
                            }
                            .into(),
                            log_coverage: super::LogCoverage::for_runner(
                                self.record.snapshot.source.profile.runner,
                            ),
                            root_exit_code: self.root_code,
                            root_signal: self.root_signal,
                            game_health: "unknown".into(),
                            client_errors: self.server.errors(),
                            logs: self.status(None).logs,
                        };
                        let mut detail = serde_json::to_string(&evidence)
                            .map_err(|e| Error::Invalid(e.to_string()))?;
                        while detail.len() > 3500 && !evidence.logs.errors.is_empty() {
                            evidence.logs.errors.pop();
                            evidence.logs.errors_omitted =
                                evidence.logs.errors_omitted.saturating_add(1);
                            detail = serde_json::to_string(&evidence)
                                .map_err(|e| Error::Invalid(e.to_string()))?;
                        }
                        self.persist(
                            if self.signal.is_some() {
                                SessionState::Stopped
                            } else {
                                SessionState::Exited
                            },
                            &detail,
                            self.root_code,
                        )?;
                    }
                    break;
                }
                if self.children.is_empty()
                    && !self.unknown
                    && self.state != SessionState::Finalizing
                    && let Err(error) = self.persist(
                        SessionState::Finalizing,
                        "Owned processes exited; draining final output.",
                        self.root_code,
                    )
                {
                    self.lose_tracking(error);
                }
            } else if matches!(
                self.state,
                SessionState::Cancelled | SessionState::LaunchFailed
            ) {
                break;
            }
            if let Err(error) = self.wait() {
                self.lose_tracking(error);
                std::thread::sleep(TICK);
            }
        }
        let deadline = Instant::now() + Duration::from_millis(100);
        while self.server.pending() && Instant::now() < deadline {
            self.server.flush();
            self.wait()
                .map_err(|e| crate::error::io("finish the session response", e))?;
        }
        Ok(())
    }
}
