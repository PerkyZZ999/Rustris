use super::{
    Client, Operation, Preflight,
    identity::{observe_process, private_runtime},
};
use crate::{
    Error, Result,
    domain::Runner,
    paths::{Paths, private_directory},
    sessions::{BaseEnvironment, SessionState},
    store::Store,
};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct LaunchOptions {
    pub helper: PathBuf,
    pub runtime: PathBuf,
    pub state: PathBuf,
}
impl LaunchOptions {
    /// Resolve owned logs and short sockets only when native launch is requested.
    pub fn discover(helper: PathBuf) -> Result<Self> {
        if !helper.is_absolute() {
            return Err(Error::Invalid(
                "The session helper path must be absolute.".into(),
            ));
        }
        let state = match std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
            Some(value) => PathBuf::from(value),
            None => PathBuf::from(std::env::var_os("HOME").ok_or_else(|| {
                Error::Invalid(
                    "An absolute HOME or XDG_STATE_HOME is needed for native logs.".into(),
                )
            })?)
            .join(".local/state"),
        };
        if !state.is_absolute() {
            return Err(Error::Invalid(
                "The native log root must be absolute.".into(),
            ));
        }
        let state = state.join("rustris");
        private_directory(&state)?;
        let preferred = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let runtime = private_runtime(preferred.as_deref())?;
        Ok(Self {
            helper,
            runtime,
            state,
        })
    }
}
pub struct LaunchOutcome {
    pub session: Uuid,
    pub preflight: Preflight,
    pub client: Option<Client>,
    pub state: SessionState,
}

/// Blocking service entry point: invoke on an application worker, never Qt's thread.
pub fn launch(
    paths: Paths,
    game: Uuid,
    revision: i64,
    base: BaseEnvironment,
    options: LaunchOptions,
    cancel: &AtomicBool,
) -> Result<LaunchOutcome> {
    begin(paths, game, revision, base, options, cancel, None)
}
/// Launch only the opaque prepared UMU command; Qt review remains a separate gate.
pub fn launch_umu(
    paths: Paths,
    game: Uuid,
    revision: i64,
    prepared: &crate::runners::umu::PreparedCommand,
    options: LaunchOptions,
    cancel: &AtomicBool,
) -> Result<LaunchOutcome> {
    begin(
        paths,
        game,
        revision,
        BaseEnvironment::default(),
        options,
        cancel,
        Some(prepared),
    )
}
fn begin(
    paths: Paths,
    game: Uuid,
    revision: i64,
    base: BaseEnvironment,
    options: LaunchOptions,
    cancel: &AtomicBool,
    prepared: Option<&crate::runners::umu::PreparedCommand>,
) -> Result<LaunchOutcome> {
    if [&options.helper, &options.runtime, &options.state]
        .iter()
        .any(|path| !path.is_absolute())
    {
        return Err(Error::Invalid(
            "Native helper paths must be absolute.".into(),
        ));
    }
    private_directory(&options.runtime)?;
    private_directory(&options.state)?;
    let mut store = Store::open_existing(paths.clone())?;
    let entry = store
        .snapshot()?
        .entries
        .into_iter()
        .find(|entry| entry.game_id == game)
        .ok_or(Error::NotFound)?;
    if (entry.profile.runner == Runner::Umu) != prepared.is_some() {
        return Err(Error::Invalid(
            "This native service does not enable the UMU adapter.".into(),
        ));
    }
    let (session, owner) = if let Some(prepared) = prepared {
        store.reserve_umu_session(game, revision, prepared)?
    } else {
        store.reserve_session(game, revision, &base)?
    };
    let preflight = match crate::sessions::execution::resolve(&session.snapshot) {
        Ok(resolved) => resolved.report,
        Err(error) => {
            store.transition_session(
                &owner,
                session.generation,
                SessionState::LaunchFailed,
                "Preflight could not complete; no helper/game was spawned.",
                None,
            )?;
            return Err(error);
        }
    };
    let prepared = store.record_preflight(&owner, session.generation, &preflight)?;
    if !preflight.findings.is_empty() || cancel.load(Ordering::Acquire) {
        let state = if cancel.load(Ordering::Acquire) {
            SessionState::Cancelled
        } else {
            SessionState::LaunchFailed
        };
        store.transition_session(&owner, prepared.generation, state, "Preparation was cancelled or blocked before helper/game spawn; see the linked preflight evidence.", None)?;
        return Ok(LaunchOutcome {
            session: session.id,
            preflight,
            client: None,
            state,
        });
    }
    let reaper = match prepare_reaper(paths.clone(), session.id) {
        Ok(reaper) => reaper,
        Err(error) => {
            store.transition_session(
                &owner,
                prepared.generation,
                SessionState::LaunchFailed,
                "Exit observation could not be allocated; no helper/game started.",
                None,
            )?;
            return Err(error);
        }
    };
    let child = Command::new(&options.helper)
        .arg(&paths.data)
        .arg(&paths.config)
        .arg(&options.runtime)
        .arg(&options.state)
        .arg(session.id.to_string())
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let child = match child {
        Ok(child) => child,
        Err(error) => {
            store.transition_session(
                &owner,
                prepared.generation,
                SessionState::LaunchFailed,
                "The native helper could not be spawned; no game started.",
                None,
            )?;
            return Err(crate::error::io("spawn the session helper", error));
        }
    };
    reaper.send(child).map_err(|_|Error::Invalid("The independent helper started, but exit observation is unavailable. Reconcile the session before retrying.".into()))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let client = loop {
        let record = store.session(session.id)?;
        if let Some(identity) = record.helper {
            break Client::new(identity, owner.clone());
        }
        if record.state != SessionState::Preparing || Instant::now() >= deadline {
            return Err(Error::Invalid("The helper did not establish its session identity. No replacement was launched; reconcile the reservation.".into()));
        }
        thread::sleep(Duration::from_millis(20));
    };
    let operation = if cancel.load(Ordering::Acquire) {
        Operation::Cancel
    } else {
        Operation::Start
    };
    let reply = client.request(operation)?;
    if !reply.accepted {
        return Err(Error::Invalid(reply.reason.unwrap_or_else(|| {
            "The helper did not accept the launch request.".into()
        })));
    }
    let state = store.session(session.id)?.state;
    Ok(LaunchOutcome {
        session: session.id,
        preflight,
        client: Some(client),
        state,
    })
}
fn prepare_reaper(paths: Paths, session: Uuid) -> Result<mpsc::SyncSender<Child>> {
    let (sender, receiver) = mpsc::sync_channel::<Child>(1);
    thread::Builder::new().name("rustris-helper-exit".into()).spawn(move||{
        let Ok(mut child)=receiver.recv()else{return;};
        if let Err(error)=child.wait(){tracing::error!(%error,"Native helper exit observation failed");return;}
        let result=(||{
            let mut store=Store::open_existing(paths)?;let record=store.session(session)?;
            if matches!(record.state,SessionState::Exited|SessionState::Stopped|SessionState::Cancelled|SessionState::LaunchFailed|SessionState::ReleasedUnknown|SessionState::Unknown){return Ok(());}
            store.mark_session_unknown(session,record.generation,"The helper exited without a durable observed completion; process outcome is unknown.")?;
            Ok::<_,Error>(())
        })();
        if let Err(error)=result{tracing::error!(%error,"Native session reconciliation failed; reservations were not released");}
    }).map_err(|e|crate::error::io("prepare helper exit observation",e))?;
    Ok(sender)
}

/// Reconnect only to the original boot/start identity and same-user socket peer.
/// No signal or reservation release follows a stale numeric PID.
pub fn reconnect(paths: Paths, session: Uuid) -> Result<Client> {
    let mut store = Store::open_existing(paths)?;
    let record = store.session(session)?;
    let identity = record.helper.ok_or_else(|| {
        Error::Invalid("The helper identity is unavailable; reconcile this reservation.".into())
    })?;
    match observe_process(identity.pid, identity.socket.clone()) {
        Ok(observed) if observed == identity => {}
        _ => {
            if !matches!(
                record.state,
                SessionState::Exited
                    | SessionState::Stopped
                    | SessionState::Cancelled
                    | SessionState::LaunchFailed
                    | SessionState::ReleasedUnknown
                    | SessionState::Unknown
            ) {
                store.mark_session_unknown(
                    session,
                    record.generation,
                    "The original helper process identity is unavailable; no PID was signaled.",
                )?;
            }
            return Err(Error::Invalid(
                "The original helper is unavailable; tracking is unknown.".into(),
            ));
        }
    }
    let client = Client::new(identity, store.session_capability(session)?);
    let reply = match client.request(Operation::Status) {
        Ok(reply) => reply,
        Err(error) => {
            if !matches!(
                record.state,
                SessionState::Exited
                    | SessionState::Stopped
                    | SessionState::Cancelled
                    | SessionState::LaunchFailed
                    | SessionState::ReleasedUnknown
                    | SessionState::Unknown
            ) {
                store.mark_session_unknown(
                    session,
                    record.generation,
                    "The original helper could not be reached/verified; no PID was signaled.",
                )?;
            }
            return Err(error);
        }
    };
    if !reply.accepted {
        return Err(Error::Invalid(
            "The helper did not accept reconnection.".into(),
        ));
    }
    if reply
        .status
        .is_some_and(|status| status.detached || status.tracking_unknown)
    {
        let attached = client.request(Operation::Attach)?;
        if !attached.accepted {
            return Err(Error::Invalid(
                attached
                    .reason
                    .unwrap_or_else(|| "The helper could not reattach safely.".into()),
            ));
        }
    }
    Ok(client)
}
