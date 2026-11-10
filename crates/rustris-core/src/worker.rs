use crate::{
    Error, Result,
    domain::ProfileInput,
    paths::{Paths, Settings},
    store::{Snapshot, Store},
};
use std::thread::{self, JoinHandle};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const CAPACITY: usize = 64;

/// Commands are owned values; no Qt object crosses this boundary.
pub enum Command {
    Load,
    Save(Option<(Uuid, i64)>, ProfileInput),
    Favorite(Uuid, bool),
    Select(Uuid),
    Remove(Uuid),
    Artwork(Uuid, Option<crate::bytes::OsBytes>),
    Preferences(Settings),
    /// D-011: record files the artwork worker downloaded.
    Fetched(Uuid, Vec<crate::artwork::Fetched>),
    /// D-011: turn automatic artwork on or off, or change the stored key.
    ArtworkSettings(crate::artwork::Change),
}

pub enum Response {
    Snapshot(Snapshot),
    SelectionSaved,
}

struct Envelope {
    command: Option<Command>,
    reply: oneshot::Sender<Result<Response>>,
}

/// Cloneable queue handle. Submission is bounded and never blocks the UI thread.
#[derive(Clone)]
pub struct Client {
    sender: mpsc::Sender<Envelope>,
}

impl Client {
    /// Accept ordered work without blocking. Dropping the reply does not cancel it.
    pub fn submit(&self, command: Command) -> Result<oneshot::Receiver<Result<Response>>> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .try_send(Envelope {
                command: Some(command),
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => Error::Invalid(
                    "Library work is queued. Wait for pending changes before retrying.".into(),
                ),
                mpsc::error::TrySendError::Closed(_) => Error::WorkerClosed,
            })?;
        Ok(receiver)
    }
}

/// Main owns shutdown and joins only after the Qt event loop has stopped.
pub struct Worker {
    client: Client,
    join: JoinHandle<()>,
}

impl Worker {
    pub fn start(paths: Paths) -> Result<Self> {
        let (sender, mut receiver) = mpsc::channel::<Envelope>(CAPACITY);
        let join = thread::Builder::new()
            .name("rustris-storage".into())
            .spawn(move || {
                let mut store = None;
                while let Some(envelope) = receiver.blocking_recv() {
                    let Some(command) = envelope.command else {
                        break;
                    };
                    let result = (|| {
                        if store.is_none() {
                            let opened = Store::open(paths.clone())?;
                            // Files a crash or a removed game left behind (D-011).
                            opened.sweep_owned_artwork();
                            store = Some(opened);
                        }
                        let storage = store.as_mut().ok_or(Error::WorkerClosed)?;
                        let changed = !matches!(command, Command::Load | Command::Select(_));
                        match command {
                            Command::Load => {}
                            Command::Save(identity, input) => {
                                storage.save(identity, &input)?;
                            }
                            Command::Favorite(id, value) => storage.favorite(id, value)?,
                            Command::Select(id) => {
                                storage.select(id)?;
                                return Ok(Response::SelectionSaved);
                            }
                            Command::Remove(id) => {
                                storage.remove(id)?;
                                // Downloaded artwork belongs to Rustris; user files are untouched.
                                let owned = crate::artwork::directory(&paths, id);
                                if let Err(error) = std::fs::remove_dir_all(&owned)
                                    && error.kind() != std::io::ErrorKind::NotFound
                                {
                                    tracing::warn!("Could not remove downloaded artwork: {error}");
                                }
                            }
                            Command::Artwork(id, path) => storage.set_artwork(id, path.as_ref())?,
                            Command::Preferences(value) => storage.preferences(value)?,
                            Command::Fetched(id, files) => {
                                record_fetched(storage, &paths, id, files)?;
                            }
                            Command::ArtworkSettings(change) => {
                                crate::artwork::change_settings(&paths, change)?;
                            }
                        }
                        storage.snapshot().map(Response::Snapshot).map_err(|error| {
                            if changed {
                                Error::CommittedRefresh(Box::new(error))
                            } else {
                                error
                            }
                        })
                    })();
                    // A removed view drops its receiver; committed state remains valid.
                    if envelope.reply.send(result).is_err() {
                        tracing::debug!("View closed before receiving its library result");
                    }
                }
            })
            .map_err(|e| crate::error::io("start the storage worker", e))?;
        Ok(Self {
            client: Client { sender },
            join,
        })
    }

    pub fn client(&self) -> Client {
        self.client.clone()
    }

    /// Drain accepted work and join. Call only after the UI event loop has stopped.
    pub fn shutdown(self) -> Result<()> {
        let (reply, _receiver) = oneshot::channel();
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
            .map_err(|_| Error::Invalid("The storage worker stopped unexpectedly.".into()));
        sent?;
        joined
    }
}

/// Record downloaded files. A file that cannot be recorded is removed again, so
/// nothing Rustris downloaded is left without an owner; if an earlier file was
/// already committed the error is reported as committed (the view must reload).
fn record_fetched(
    storage: &mut Store,
    paths: &Paths,
    game: Uuid,
    files: Vec<crate::artwork::Fetched>,
) -> Result<()> {
    let mut committed = false;
    let mut failure = None;
    for file in files {
        let path = crate::bytes::OsBytes::new(std::os::unix::ffi::OsStrExt::as_bytes(
            file.path.as_os_str(),
        ));
        let result = path.and_then(|path| {
            storage
                .set_fetched_artwork(game, file.kind, &path, file.source)
                .map(|replaced| (path, replaced))
        });
        match result {
            Ok((path, replaced)) => {
                committed = true;
                if let Some(old) = replaced.filter(|old| old != &path)
                    && !storage.is_local_artwork(&old)
                {
                    crate::artwork::remove_owned(paths, &old.path());
                }
            }
            Err(error) => {
                crate::artwork::remove_owned(paths, &file.path);
                if matches!(error, Error::NotFound) {
                    // The game was removed while its artwork downloaded: nothing to show.
                    let _ = std::fs::remove_dir_all(crate::artwork::directory(paths, game));
                    continue;
                }
                failure.get_or_insert(error);
            }
        }
    }
    match failure {
        None => Ok(()),
        Some(error) if committed => Err(Error::CommittedRefresh(Box::new(error))),
        Some(error) => Err(error),
    }
}
