//! Qt-free library/profile domain, private persistence, and serialized worker.

pub mod artwork;
pub mod bytes;
pub mod domain;
pub mod error;
mod migrations;
pub mod paths;
mod private_config;
pub mod runners;
pub mod sessions;
pub mod store;
pub mod worker;

pub use error::{Error, Result};
