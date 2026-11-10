use super::identity::observe_process;
use crate::{
    Error, Result,
    sessions::{HelperIdentity, SessionOwner},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(super) const VERSION: u16 = 2;
pub(super) const REQUEST_LIMIT: usize = 4096;
pub(super) const RESPONSE_LIMIT: usize = 256 * 1024;
const IO_BUDGET: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "WireOperation", into = "WireOperation")]
pub enum Operation {
    Status,
    Start,
    Cancel,
    Stop,
    Force,
    Detach,
    Attach,
    Logs { after: u64 },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Status,
    Start,
    Cancel,
    Stop,
    Force,
    Detach,
    Attach,
    Logs,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireOperation {
    kind: Kind,
    #[serde(skip_serializing_if = "Option::is_none")]
    after: Option<u64>,
}
impl TryFrom<WireOperation> for Operation {
    type Error = &'static str;
    fn try_from(wire: WireOperation) -> std::result::Result<Self, Self::Error> {
        if let Kind::Logs = wire.kind {
            return wire
                .after
                .map(|after| Self::Logs { after })
                .ok_or("A log cursor is required");
        }
        if wire.after.is_some() {
            return Err("Only a log request accepts a cursor");
        }
        Ok(match wire.kind {
            Kind::Status => Self::Status,
            Kind::Start => Self::Start,
            Kind::Cancel => Self::Cancel,
            Kind::Stop => Self::Stop,
            Kind::Force => Self::Force,
            Kind::Detach => Self::Detach,
            Kind::Attach => Self::Attach,
            Kind::Logs => return Err("A log cursor is required"),
        })
    }
}
impl From<Operation> for WireOperation {
    fn from(operation: Operation) -> Self {
        let kind = match operation {
            Operation::Status => Kind::Status,
            Operation::Start => Kind::Start,
            Operation::Cancel => Kind::Cancel,
            Operation::Stop => Kind::Stop,
            Operation::Force => Kind::Force,
            Operation::Detach => Kind::Detach,
            Operation::Attach => Kind::Attach,
            Operation::Logs { after } => {
                return Self {
                    kind: Kind::Logs,
                    after: Some(after),
                };
            }
        };
        Self { kind, after: None }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub version: u16,
    pub session: Uuid,
    pub secret: Uuid,
    pub operation: Operation,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reply {
    pub version: u16,
    pub session: Uuid,
    pub accepted: bool,
    pub reason: Option<String>,
    pub status: Option<super::status::NativeStatus>,
}

/// A peer-checked, bounded local capability. Its secret is never a presentation value.
#[derive(Clone)]
pub struct Client {
    identity: HelperIdentity,
    owner: SessionOwner,
}
impl Client {
    pub fn new(identity: HelperIdentity, owner: SessionOwner) -> Self {
        Self { identity, owner }
    }
    pub fn request(&self, operation: Operation) -> Result<Reply> {
        let observed = observe_process(self.identity.pid, self.identity.socket.clone())?;
        if observed != self.identity {
            return Err(Error::Invalid(
                "The original helper identity is no longer available; tracking is unknown.".into(),
            ));
        }
        let descriptor = rustix::net::socket_with(
            rustix::net::AddressFamily::UNIX,
            rustix::net::SocketType::STREAM,
            rustix::net::SocketFlags::CLOEXEC | rustix::net::SocketFlags::NONBLOCK,
            None,
        )
        .map_err(|e| crate::error::io("create a helper connection", e.into()))?;
        let address = rustix::net::SocketAddrUnix::new(self.identity.socket.path())
            .map_err(|e| crate::error::io("validate the helper socket path", e.into()))?;
        match rustix::net::connect(&descriptor, &address) {
            Ok(()) => {}
            Err(error) => {
                return Err(crate::error::io(
                    "connect to the session helper",
                    error.into(),
                ));
            }
        }
        let mut stream = UnixStream::from(descriptor);
        stream
            .set_nonblocking(false)
            .map_err(|e| crate::error::io("configure the helper connection", e))?;
        let credentials = rustix::net::sockopt::socket_peercred(&stream)
            .map_err(|e| crate::error::io("verify the helper peer", e.into()))?;
        if credentials.uid != rustix::process::geteuid()
            || credentials.pid.as_raw_pid() as u32 != self.identity.pid
        {
            return Err(Error::Invalid(
                "The helper peer identity was not accepted.".into(),
            ));
        }
        let deadline = Instant::now() + IO_BUDGET;
        let request = Request {
            version: VERSION,
            session: self.owner.id(),
            secret: self.owner.secret(),
            operation,
        };
        let body = serde_json::to_vec(&request).map_err(|e| Error::Invalid(e.to_string()))?;
        if body.len() > REQUEST_LIMIT {
            return Err(Error::Invalid(
                "The helper request exceeds its frame limit.".into(),
            ));
        }
        stream
            .set_write_timeout(Some(IO_BUDGET))
            .map_err(|e| crate::error::io("configure the helper write deadline", e))?;
        stream
            .write_all(&(body.len() as u32).to_be_bytes())
            .and_then(|_| stream.write_all(&body))
            .map_err(|e| crate::error::io("write the helper request", e))?;
        let mut header = [0; 4];
        read_deadline(&mut stream, &mut header, deadline)?;
        let length = u32::from_be_bytes(header) as usize;
        if length > RESPONSE_LIMIT {
            return Err(Error::Invalid(
                "The helper response exceeds its frame limit.".into(),
            ));
        }
        let mut payload = vec![0; length];
        read_deadline(&mut stream, &mut payload, deadline)?;
        let reply: Reply = serde_json::from_slice(&payload)
            .map_err(|_| Error::Invalid("The helper response is malformed.".into()))?;
        if reply.version != VERSION || reply.session != self.owner.id() {
            return Err(Error::Invalid(
                "The helper response identity/version was not accepted.".into(),
            ));
        }
        Ok(reply)
    }
}
fn read_deadline(
    stream: &mut UnixStream,
    mut destination: &mut [u8],
    deadline: Instant,
) -> Result<()> {
    while !destination.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| Error::Invalid("The helper response deadline elapsed.".into()))?;
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|e| crate::error::io("configure the helper read deadline", e))?;
        let count = stream
            .read(destination)
            .map_err(|e| crate::error::io("read the helper response", e))?;
        if count == 0 {
            return Err(Error::Invalid(
                "The helper closed its response early.".into(),
            ));
        }
        destination = &mut destination[count..];
    }
    Ok(())
}
