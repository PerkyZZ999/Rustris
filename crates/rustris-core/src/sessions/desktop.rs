//! Process-owned bounded native-session worker. No UI lifetime owns accepted work.
mod presentation;
mod umu_review;
pub use super::overview::Overview;
use super::{
    BaseEnvironment, SessionState,
    native::{self, LaunchOptions, Operation},
};
use crate::{Error, Result, paths::Paths, store::Store};
pub use presentation::Details;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};
use tokio::sync::{mpsc, oneshot};
pub use umu_review::Review;
use uuid::Uuid;

pub enum Action {
    Stop,
    Force,
    Reconnect,
    ReleaseUnknown { generation: i64, acknowledged: bool },
}
pub enum Command {
    Refresh {
        game: Option<Uuid>,
        session: Option<Uuid>,
        reload_logs: bool,
    },
    Play {
        game: Uuid,
        revision: i64,
        cancel: Arc<AtomicBool>,
    },
    Control {
        session: Uuid,
        action: Action,
    },
    ReadRunnerSelection,
    SaveRunnerSelection(crate::runners::umu::Selection),
    PreviewUmu {
        game: Uuid,
        revision: i64,
    },
    ConfirmUmu {
        ticket: Uuid,
        cancel: Arc<AtomicBool>,
    },
    DiscardUmu {
        ticket: Uuid,
    },
    LeaveRunning,
}
#[derive(Default)]
pub enum Effect {
    #[default]
    None,
    RunnerLoaded(
        Option<crate::runners::umu::Selection>,
        crate::runners::detect::Detected,
    ),
    RunnerSaved,
    ReviewReady,
}
#[derive(Default, Serialize)]
pub struct Snapshot {
    pub sessions: Vec<Overview>,
    pub selected: Option<Details>,
    pub active_count: usize,
    pub close_ready: bool,
    pub review: Option<Review>,
    #[serde(skip)]
    pub effect: Effect,
}
struct Envelope {
    command: Option<Command>,
    reply: oneshot::Sender<Result<Snapshot>>,
}
#[derive(Clone)]
pub struct Client {
    sender: mpsc::Sender<Envelope>,
}
impl Client {
    pub fn submit(&self, command: Command) -> Result<oneshot::Receiver<Result<Snapshot>>> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(Envelope {
                command: Some(command),
                reply,
            })
            .map_err(|e| match e {
                mpsc::error::TrySendError::Full(_) => {
                    Error::Invalid("Session work is queued. Wait before retrying.".into())
                }
                mpsc::error::TrySendError::Closed(_) => Error::WorkerClosed,
            })?;
        Ok(receiver)
    }
}
pub struct Worker {
    client: Client,
    join: JoinHandle<()>,
}
impl Worker {
    pub fn start(paths: Paths, helper: PathBuf) -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel::<Envelope>(16);
        let join=thread::Builder::new().name("rustris-sessions".into()).spawn(move|| {
            let mut service=Service{paths,helper,store:None,clients:BTreeMap::new(),game:None,selected:None,tail:presentation::Tail::default(),left:false,review:None};
            while let Some(envelope)=receiver.blocking_recv() {
                let Some(command)=envelope.command else {
                    if !service.left && let Err(error)=service.leave_running(){tracing::error!(%error,"Session shutdown could not acknowledge every helper; reservations remain");}
                    break;
                };
                let result=service.execute(command);
                if envelope.reply.send(result).is_err(){tracing::debug!("Session view closed before delivery; accepted work completed");}
            }
        }).map_err(|e|crate::error::io("start the session worker",e))?;
        Ok(Self {
            client: Client { sender },
            join,
        })
    }
    pub fn client(&self) -> Client {
        self.client.clone()
    }
    pub fn shutdown(self) -> Result<()> {
        let (reply, _) = oneshot::channel();
        let sent = self
            .client
            .sender
            .blocking_send(Envelope {
                command: None,
                reply,
            })
            .map_err(|_| Error::WorkerClosed);
        let joined = self
            .join
            .join()
            .map_err(|_| Error::Invalid("The session worker stopped unexpectedly.".into()));
        sent?;
        joined
    }
}
struct Service {
    paths: Paths,
    helper: PathBuf,
    store: Option<Store>,
    clients: BTreeMap<Uuid, native::Client>,
    game: Option<Uuid>,
    selected: Option<Uuid>,
    tail: presentation::Tail,
    left: bool,
    review: Option<umu_review::Pending>,
}
impl Service {
    fn storage(&mut self) -> Result<&mut Store> {
        if self.store.is_none() {
            self.store = Some(Store::open_existing(self.paths.clone())?);
        }
        self.store.as_mut().ok_or(Error::WorkerClosed)
    }
    fn execute(&mut self, command: Command) -> Result<Snapshot> {
        self.left = false;
        let mut effect = Effect::None;
        match command {
            Command::Refresh {
                game,
                session,
                reload_logs,
            } => {
                if reload_logs {
                    self.tail.reload();
                }
                if self.game != game {
                    self.review = None;
                }
                self.game = game;
                self.selected = session;
            }
            Command::Play {
                game,
                revision,
                cancel,
            } => {
                self.game = Some(game);
                let options = LaunchOptions::discover(self.helper.clone())?;
                let launched = native::launch(
                    self.paths.clone(),
                    game,
                    revision,
                    BaseEnvironment::capture()?,
                    options,
                    &cancel,
                )?;
                self.selected = Some(launched.session);
                if let Some(client) = launched.client {
                    self.clients.insert(launched.session, client);
                }
                // A cancellation racing accepted Start never becomes an implicit Stop.
                if cancel.load(Ordering::Acquire) && launched.state != SessionState::Cancelled {
                    tracing::debug!(
                        "Cancellation arrived after spawn; Stop remains an explicit action"
                    );
                }
            }
            Command::Control { session, action } => {
                self.selected = Some(session);
                match action {
                    Action::ReleaseUnknown {
                        generation,
                        acknowledged,
                    } => {
                        self.storage()?.release_unknown_session(
                            session,
                            generation,
                            acknowledged,
                        )?;
                        self.clients.remove(&session);
                    }
                    Action::Reconnect => {
                        self.clients.remove(&session);
                        let client = self.connect(session)?;
                        self.clients.insert(session, client);
                    }
                    Action::Stop | Action::Force => {
                        let client = self.client(session)?;
                        let operation = if matches!(action, Action::Force) {
                            Operation::Force
                        } else {
                            Operation::Stop
                        };
                        let reply = client.request(operation)?;
                        if !reply.accepted {
                            return Err(Error::Invalid(
                                reply
                                    .reason
                                    .unwrap_or_else(|| "The session operation was refused.".into()),
                            ));
                        }
                    }
                }
            }
            Command::ReadRunnerSelection => {
                let detected = match self.paths.data.parent() {
                    Some(data_home) => crate::runners::detect::detect(
                        data_home,
                        std::env::var_os("HOME")
                            .as_deref()
                            .map(std::path::Path::new),
                        std::env::var_os("PATH").as_ref(),
                    ),
                    None => crate::runners::detect::Detected::default(),
                };
                effect = Effect::RunnerLoaded(self.paths.read_runner_selection()?, detected)
            }
            Command::SaveRunnerSelection(selection) => {
                self.paths.write_runner_selection(&selection)?;
                self.review = None;
                // Fingerprints already cover the runner; clearing keeps the Play
                // label honest until the next review (D-010).
                if let Err(error) = self
                    .storage()
                    .and_then(crate::store::Store::clear_approved_reviews)
                {
                    tracing::warn!("Could not clear approved launch reviews: {error}");
                }
                effect = Effect::RunnerSaved;
            }
            Command::PreviewUmu { game, revision } => {
                self.review = None;
                self.game = Some(game);
                self.review = Some(self.preview_umu(game, revision)?);
                effect = Effect::ReviewReady;
            }
            Command::ConfirmUmu { ticket, cancel } => {
                if self
                    .review
                    .as_ref()
                    .is_none_or(|review| review.view.ticket != ticket)
                {
                    return Err(Error::Invalid(
                        "The launch review changed. Review the profile again.".into(),
                    ));
                }
                let pending = self.review.take().ok_or(Error::SessionChanged)?;
                let plan = pending.plan.ok_or_else(|| {
                    Error::Invalid(
                        "Preparation is incomplete. Review the findings before launching.".into(),
                    )
                })?;
                if let Some(fingerprint) = pending.fingerprint {
                    // The approval only saves a later dialog; failing to record it
                    // must not block the launch the user just confirmed.
                    if let Err(error) = self
                        .storage()
                        .and_then(|store| store.approve_review(pending.game, &fingerprint))
                    {
                        tracing::warn!("Could not remember the approved launch review: {error}");
                    }
                }
                let options = LaunchOptions::discover(self.helper.clone())?;
                let launched = native::launch_umu(
                    self.paths.clone(),
                    pending.game,
                    pending.revision,
                    &plan,
                    options,
                    &cancel,
                )?;
                self.selected = Some(launched.session);
                if let Some(client) = launched.client {
                    self.clients.insert(launched.session, client);
                }
                if cancel.load(Ordering::Acquire) && launched.state != SessionState::Cancelled {
                    tracing::debug!("Cancellation arrived after Start; Stop remains explicit");
                }
            }
            Command::DiscardUmu { ticket } => {
                if self
                    .review
                    .as_ref()
                    .is_some_and(|review| review.view.ticket == ticket)
                {
                    self.review = None;
                }
            }
            Command::LeaveRunning => {
                self.review = None;
                self.leave_running()?;
                self.left = true;
                return Ok(Snapshot {
                    close_ready: true,
                    ..Snapshot::default()
                });
            }
        }
        let mut snapshot = self.snapshot()?;
        snapshot.effect = effect;
        Ok(snapshot)
    }
    fn connect(&mut self, session: Uuid) -> Result<native::Client> {
        // A concurrent controller may still be allocating the helper. Allow its
        // bounded preparation window before treating a missing identity as unknown.
        let record = self.storage()?.session(session)?;
        if record.helper.is_none()
            && record.state == SessionState::Preparing
            && super::now_ms()?.saturating_sub(self.created(session)?) > 30_000
        {
            self.storage()?.mark_session_unknown(session,record.generation,"Preparation has no original helper identity after its deadline. No process was signaled.")?;
        }
        native::reconnect(self.paths.clone(), session)
    }
    fn created(&mut self, session: Uuid) -> Result<i64> {
        self.storage()?
            .connection
            .query_row(
                "SELECT created_ms FROM launch_sessions WHERE id=?1",
                [session.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .map_err(Into::into)
    }
    fn client(&mut self, session: Uuid) -> Result<native::Client> {
        if let Some(client) = self.clients.get(&session) {
            return Ok(client.clone());
        }
        let client = self.connect(session)?;
        self.clients.insert(session, client.clone());
        Ok(client)
    }
    fn leave_running(&mut self) -> Result<()> {
        if self.store.is_none() && !self.paths.database().exists() {
            return Ok(());
        }
        let records = self.storage()?.session_overview(None)?;
        for record in records.into_iter().filter(|r| r.reserved) {
            let client = match self.client(record.id) {
                Ok(client) => client,
                Err(error) => {
                    let current = self.storage()?.session(record.id)?;
                    if current.state == SessionState::Unknown {
                        continue;
                    }
                    return Err(error);
                }
            };
            let reply = client.request(Operation::Detach)?;
            if !reply.accepted {
                let current = self.storage()?.session(record.id)?;
                if !current.state.observed_terminal()
                    && current.state != SessionState::ReleasedUnknown
                    && current.state != SessionState::Unknown
                {
                    return Err(Error::Invalid(reply.reason.unwrap_or_else(|| {
                        "The helper did not acknowledge leaving it running.".into()
                    })));
                }
            }
        }
        self.clients.clear();
        Ok(())
    }
    fn snapshot(&mut self) -> Result<Snapshot> {
        let game = self.game;
        let sessions = self.storage()?.session_overview(game)?;
        self.clients
            .retain(|id, _| sessions.iter().any(|r| r.id == *id && r.reserved));
        let active_count = sessions.iter().filter(|r| r.reserved).count();
        let selected = self
            .selected
            .filter(|id| sessions.iter().any(|r| r.id == *id))
            .or_else(|| sessions.iter().find(|r| Some(r.game) == game).map(|r| r.id));
        self.selected = selected;
        let details = selected.map(|id| self.details(id)).transpose()?;
        Ok(Snapshot {
            sessions,
            selected: details,
            active_count,
            close_ready: false,
            review: self.review.as_ref().map(|pending| pending.view.clone()),
            effect: Effect::None,
        })
    }
}
