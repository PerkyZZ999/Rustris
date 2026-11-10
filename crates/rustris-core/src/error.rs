use std::io;

/// An actionable failure; failed operations never imply a successful commit.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error("Cannot {action}: {source}")]
    Io {
        action: &'static str,
        source: io::Error,
    },
    #[error("Library storage: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}. The existing database was preserved.")]
    Database(String),
    #[error("This profile changed since it was opened. Reload it before saving.")]
    Conflict,
    #[error(
        "This profile or prefix has an active or unknown session. Reconcile it before launching or changing its configuration."
    )]
    SessionBusy,
    #[error("The session changed since it was read. Refresh its state before retrying.")]
    SessionChanged,
    #[error("This library entry no longer exists. Reload the library.")]
    NotFound,
    #[error("The library worker is unavailable. Restart Rustris to reconnect.")]
    WorkerClosed,
    #[error(
        "Changes were committed, but the library could not be refreshed: {0}. Reload before making further changes."
    )]
    CommittedRefresh(Box<Error>),
}

/// Core operation result.
pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn io(action: &'static str, source: io::Error) -> Error {
    Error::Io { action, source }
}
