//! Durable launch snapshots and conservative reservations. No process signaling lives here.
pub mod desktop;
mod environment;
mod execution;
pub mod native;
mod overview;
mod runner_storage;
mod storage;
use crate::{Error, Result, bytes::OsBytes, domain::Entry};
pub use environment::BaseEnvironment;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;
const ENVIRONMENT_POLICY: &str = "desktop-allowlist-v1";
const MAX_DETAIL: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Preparing,
    Running,
    StopRequested,
    ForceRequested,
    Finalizing,
    Exited,
    Stopped,
    Cancelled,
    LaunchFailed,
    Unknown,
    ReleasedUnknown,
}
impl SessionState {
    pub fn key(self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::StopRequested => "stop_requested",
            Self::ForceRequested => "force_requested",
            Self::Finalizing => "finalizing",
            Self::Exited => "exited",
            Self::Stopped => "stopped",
            Self::Cancelled => "cancelled",
            Self::LaunchFailed => "launch_failed",
            Self::Unknown => "unknown",
            Self::ReleasedUnknown => "released_unknown",
        }
    }
    fn parse(value: &str) -> Result<Self> {
        [
            Self::Preparing,
            Self::Running,
            Self::StopRequested,
            Self::ForceRequested,
            Self::Finalizing,
            Self::Exited,
            Self::Stopped,
            Self::Cancelled,
            Self::LaunchFailed,
            Self::Unknown,
            Self::ReleasedUnknown,
        ]
        .into_iter()
        .find(|state| state.key() == value)
        .ok_or_else(|| Error::Database("A session has an unrecognized state".into()))
    }
    fn observed_terminal(self) -> bool {
        matches!(
            self,
            Self::Exited | Self::Stopped | Self::Cancelled | Self::LaunchFailed
        )
    }
    fn permits(self, next: Self) -> bool {
        match self {
            Self::Preparing => matches!(
                next,
                Self::Running
                    | Self::Finalizing
                    | Self::Cancelled
                    | Self::LaunchFailed
                    | Self::Unknown
            ),
            Self::Running => matches!(next, Self::StopRequested | Self::Finalizing | Self::Unknown),
            Self::StopRequested => matches!(
                next,
                Self::ForceRequested | Self::Finalizing | Self::Unknown
            ),
            Self::ForceRequested => matches!(next, Self::Finalizing | Self::Unknown),
            Self::Finalizing => matches!(next, Self::Exited | Self::Stopped | Self::Unknown),
            Self::Unknown => matches!(next, Self::Running | Self::Finalizing),
            _ => false,
        }
    }
}

/// Never use this persisted identity to signal a PID. It is reconnection evidence only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperIdentity {
    pub pid: u32,
    pub start_ticks: u64,
    pub boot_id: Uuid,
    pub socket: OsBytes,
}

/// Private local handshake capability; Debug deliberately excludes its random secret.
#[derive(Clone)]
pub struct SessionOwner {
    id: Uuid,
    secret: Uuid,
}
impl std::fmt::Debug for SessionOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionOwner")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}
impl SessionOwner {
    pub fn from_parts(id: Uuid, secret: Uuid) -> Self {
        Self { id, secret }
    }
    pub fn id(&self) -> Uuid {
        self.id
    }
    /// Sensitive local wire value; never include this in presentation DTOs or logs.
    pub fn secret(&self) -> Uuid {
        self.secret
    }
}
#[derive(Clone, Debug)]
pub struct LaunchSnapshot {
    pub source: Entry,
    pub environment: Vec<(OsBytes, OsBytes)>,
    pub environment_policy: String,
    pub runtime: Option<RunnerSnapshot>,
}
#[derive(Clone, Debug)]
pub struct RunnerSnapshot {
    pub selection: crate::runners::umu::Selection,
    pub umu_version: String,
    pub proton_version: String,
    pub runtime_version: String,
    pub policy: String,
    pub prefix: crate::runners::umu::PrefixIdentity,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: Uuid,
    pub snapshot: LaunchSnapshot,
    pub state: SessionState,
    pub generation: i64,
    pub detail: String,
    pub exit_code: Option<i32>,
    pub helper: Option<HelperIdentity>,
}

pub(crate) fn now_ms() -> Result<i64> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Invalid("The system clock is before the Unix epoch.".into()))?
            .as_millis(),
    )
    .map_err(|_| Error::Invalid("The system clock exceeds the supported range.".into()))
}
fn valid_detail(detail: &str) -> Result<()> {
    if detail.len() > MAX_DETAIL || detail.contains('\0') {
        Err(Error::Invalid(
            "A session message exceeds its bounded text limit.".into(),
        ))
    } else {
        Ok(())
    }
}
fn id(bytes: Vec<u8>) -> Result<Uuid> {
    Uuid::from_slice(&bytes).map_err(|_| Error::Database("A session identity is invalid".into()))
}
