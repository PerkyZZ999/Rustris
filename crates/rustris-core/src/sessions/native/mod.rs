//! Native helper transport and runtime. Saved PIDs never authorize signals.
mod capture;
mod controller;
mod history;
mod identity;
mod preflight;
mod protocol;
mod server;
mod sink;
mod status;
mod supervisor;
pub use capture::{LogRecord, LogStats};
pub use controller::{LaunchOptions, LaunchOutcome, launch, launch_umu, reconnect};
pub use history::{CompletedLog, read_completed};
pub use identity::{observe_process, private_runtime};
pub use preflight::{Finding, Observation, Preflight};
pub use protocol::{Client, Operation, Reply};
pub use status::{CompletionEvidence, LogCoverage, NativeStatus};

pub fn run_helper(mut arguments: impl Iterator<Item = std::ffi::OsString>) -> crate::Result<()> {
    let mut paths = Vec::new();
    for _ in 0..4 {
        paths.push(std::path::PathBuf::from(arguments.next().ok_or_else(
            || {
                crate::Error::Invalid(
                    "Expected data, config, runtime, state and session arguments.".into(),
                )
            },
        )?));
    }
    let session = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| uuid::Uuid::parse_str(&value).ok())
        .ok_or_else(|| crate::Error::Invalid("A valid session identity is required.".into()))?;
    if arguments.next().is_some() || paths.iter().any(|path| !path.is_absolute()) {
        return Err(crate::Error::Invalid(
            "Helper paths must be absolute and arguments exact.".into(),
        ));
    }
    let library = crate::paths::Paths {
        data: paths[0].clone(),
        config: paths[1].clone(),
    };
    let mut helper =
        supervisor::Supervisor::open(library, paths[2].clone(), paths[3].clone(), session)?;
    helper.run()
}
