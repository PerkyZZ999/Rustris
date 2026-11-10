//! Qt presentation boundary; filesystem and database work belongs to the core worker.

mod backend;
mod draft;
mod engine;
mod runner_draft;
mod sessions;
#[cfg(feature = "test-support")]
mod test_support;
pub use engine::install_artwork_provider;
pub use engine::limit_image_allocations;
pub use engine::loaded as engine_loaded;
#[cfg(feature = "test-support")]
pub use test_support::install as install_test_driver;

use rustris_core::{
    Result,
    paths::Paths,
    worker::{Client, Worker},
};
use std::sync::OnceLock;

static CLIENT: OnceLock<Client> = OnceLock::new();
static SESSION_CLIENT: OnceLock<rustris_core::sessions::desktop::Client> = OnceLock::new();
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static FETCH_CLIENT: OnceLock<rustris_core::artwork::FetchClient> = OnceLock::new();

/// Establish the process-owned worker before constructing QML objects.
pub struct Workers {
    library: Worker,
    sessions: rustris_core::sessions::desktop::Worker,
    artwork: rustris_core::artwork::Fetcher,
}
impl Workers {
    pub fn shutdown(self) -> Result<()> {
        let artwork = self.artwork.shutdown();
        let sessions = self.sessions.shutdown();
        let library = self.library.shutdown();
        artwork?;
        sessions?;
        library
    }
}

pub fn initialize() -> Result<Workers> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()
        .map_err(|e| {
            rustris_core::Error::Invalid(format!("Cannot start application workers: {e}"))
        })?;
    RUNTIME.set(runtime).map_err(|_| {
        rustris_core::Error::Invalid("Application workers are already initialized.".into())
    })?;
    let worker = Worker::start(Paths::discover()?)?;
    CLIENT
        .set(worker.client())
        .map_err(|_| rustris_core::Error::Invalid("The library is already initialized.".into()))?;
    let helper = std::env::current_exe()
        .map_err(|e| {
            rustris_core::Error::Invalid(format!("Cannot locate the desktop executable: {e}"))
        })?
        .with_file_name("rustris-session");
    let sessions = rustris_core::sessions::desktop::Worker::start(Paths::discover()?, helper)?;
    SESSION_CLIENT.set(sessions.client()).map_err(|_| {
        rustris_core::Error::Invalid("The session worker is already initialized.".into())
    })?;
    let artwork = rustris_core::artwork::Fetcher::start(Paths::discover()?)?;
    if let Some(client) = artwork.client() {
        FETCH_CLIENT.set(client).map_err(|_| {
            rustris_core::Error::Invalid("The artwork worker is already initialized.".into())
        })?;
    }
    Ok(Workers {
        library: worker,
        sessions,
        artwork,
    })
}
