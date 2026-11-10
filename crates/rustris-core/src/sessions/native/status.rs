use super::capture::{LogRecord, LogStats};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogCoverage {
    #[default]
    NativeStreams,
    RunnerStreamsApplicationUnverified,
}
impl LogCoverage {
    pub(crate) fn for_runner(runner: crate::domain::Runner) -> Self {
        match runner {
            crate::domain::Runner::Native => Self::NativeStreams,
            crate::domain::Runner::Umu => Self::RunnerStreamsApplicationUnverified,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionEvidence {
    pub client_errors: u64,
    pub kind: String,
    pub root_exit_code: Option<i32>,
    pub root_signal: Option<i32>,
    pub game_health: String,
    pub logs: LogStats,
    #[serde(default)]
    pub log_coverage: LogCoverage,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStatus {
    pub state: String,
    pub durable_state: String,
    pub generation: i64,
    pub root_pid: Option<u32>,
    pub owned_children: usize,
    pub root_exit_code: Option<i32>,
    pub root_signal: Option<i32>,
    pub spawned: bool,
    pub detached: bool,
    pub tracking_unknown: bool,
    pub can_stop: bool,
    pub can_force: bool,
    pub error: Option<String>,
    pub game_health: String,
    pub logs: LogStats,
    pub records: Vec<LogRecord>,
    pub log_gap: bool,
    pub client_errors: u64,
    #[serde(default)]
    pub log_coverage: LogCoverage,
}
