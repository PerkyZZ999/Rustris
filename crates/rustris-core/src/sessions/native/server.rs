use super::protocol::{Operation, REQUEST_LIMIT, RESPONSE_LIMIT, Reply, Request, VERSION};
use crate::sessions::SessionOwner;
use rustix::event::{PollFd, PollFlags};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    os::unix::{
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const CONNECTIONS: usize = 16;
const DEADLINE: Duration = Duration::from_millis(500);
struct Connection {
    stream: UnixStream,
    input: Vec<u8>,
    output: Option<Vec<u8>>,
    written: usize,
    deadline: Instant,
}
struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        match fs::symlink_metadata(&self.path) {
            Ok(meta)
                if meta.file_type().is_socket()
                    && meta.dev() == self.device
                    && meta.ino() == self.inode =>
            {
                if let Err(error) = fs::remove_file(&self.path) {
                    tracing::error!(%error,"Could not remove the owned session socket");
                }
            }
            Ok(_) => tracing::error!("The session socket changed identity; preserving it"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::error!(%error,"Could not inspect the session socket for cleanup")
            }
        }
    }
}
pub(super) struct Server {
    listener: UnixListener,
    owner: SessionOwner,
    clients: BTreeMap<u64, Connection>,
    next: u64,
    errors: u64,
    _socket: SocketGuard,
}
impl Server {
    pub fn bind(path: &Path, owner: SessionOwner) -> io::Result<Self> {
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let meta = fs::symlink_metadata(path)?;
        Ok(Self {
            listener,
            owner,
            clients: BTreeMap::new(),
            next: 1,
            errors: 0,
            _socket: SocketGuard {
                path: path.into(),
                device: meta.dev(),
                inode: meta.ino(),
            },
        })
    }
    pub fn descriptors(&self) -> Vec<PollFd<'_>> {
        let mut descriptors = vec![PollFd::new(&self.listener, PollFlags::IN)];
        descriptors.extend(self.clients.values().map(|connection| {
            PollFd::new(
                &connection.stream,
                if connection.output.is_some() {
                    PollFlags::OUT
                } else {
                    PollFlags::IN
                },
            )
        }));
        descriptors
    }
    pub fn collect(&mut self) -> io::Result<Vec<(u64, Operation)>> {
        for _ in 0..CONNECTIONS {
            let (stream, _) = match self.listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error),
            };
            let credentials = rustix::net::sockopt::socket_peercred(&stream)?;
            if credentials.uid != rustix::process::geteuid() || self.clients.len() >= CONNECTIONS {
                self.errors = self.errors.saturating_add(1);
                continue;
            }
            stream.set_nonblocking(true)?;
            let id = self.next;
            self.next = self.next.wrapping_add(1);
            self.clients.insert(
                id,
                Connection {
                    stream,
                    input: Vec::with_capacity(REQUEST_LIMIT + 4),
                    output: None,
                    written: 0,
                    deadline: Instant::now() + DEADLINE,
                },
            );
        }
        let mut requests = Vec::new();
        let mut rejected = Vec::new();
        let mut removed = Vec::new();
        for (&id, connection) in &mut self.clients {
            if Instant::now() >= connection.deadline {
                self.errors = self.errors.saturating_add(1);
                removed.push(id);
                continue;
            }
            if connection.output.is_some() {
                continue;
            }
            let mut buffer = [0; 1024];
            for _ in 0..5 {
                let remaining = (REQUEST_LIMIT + 4).saturating_sub(connection.input.len());
                if remaining == 0 {
                    break;
                }
                match connection.stream.read(&mut buffer[..remaining.min(1024)]) {
                    Ok(0) => {
                        removed.push(id);
                        break;
                    }
                    Ok(count) => connection.input.extend_from_slice(&buffer[..count]),
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        self.errors = self.errors.saturating_add(1);
                        removed.push(id);
                        break;
                    }
                }
                if connection.input.len() > REQUEST_LIMIT + 4 {
                    rejected.push(id);
                    break;
                }
            }
            if connection.input.len() < 4 {
                continue;
            }
            let header: [u8; 4] = connection.input[..4]
                .try_into()
                .map_err(|_| io::Error::other("Frame header unavailable"))?;
            let length = u32::from_be_bytes(header) as usize;
            if length > REQUEST_LIMIT {
                rejected.push(id);
                continue;
            }
            if connection.input.len() < length + 4 {
                continue;
            }
            if connection.input.len() != length + 4 {
                rejected.push(id);
                continue;
            }
            let request: Request = match serde_json::from_slice(&connection.input[4..]) {
                Ok(request) => request,
                Err(_) => {
                    rejected.push(id);
                    continue;
                }
            };
            if request.version != VERSION
                || request.session != self.owner.id()
                || request.secret != self.owner.secret()
            {
                rejected.push(id);
                continue;
            }
            requests.push((id, request.operation));
        }
        for id in removed {
            self.clients.remove(&id);
        }
        rejected.sort_unstable();
        rejected.dedup();
        for id in rejected {
            self.errors = self.errors.saturating_add(1);
            self.respond(
                id,
                Reply {
                    version: VERSION,
                    session: self.owner.id(),
                    accepted: false,
                    reason: Some(
                        "The request identity, version or bounded frame was not accepted.".into(),
                    ),
                    status: None,
                },
            )?;
        }
        Ok(requests)
    }
    pub fn respond(&mut self, id: u64, reply: Reply) -> io::Result<()> {
        let Some(connection) = self.clients.get_mut(&id) else {
            return Ok(());
        };
        let body = serde_json::to_vec(&reply).map_err(io::Error::other)?;
        if body.len() > RESPONSE_LIMIT {
            self.clients.remove(&id);
            return Err(io::Error::other("Response exceeds its bounded frame limit"));
        }
        let mut frame = Vec::with_capacity(body.len() + 4);
        frame.extend_from_slice(&(body.len() as u32).to_be_bytes());
        frame.extend_from_slice(&body);
        connection.output = Some(frame);
        Ok(())
    }
    pub fn flush(&mut self) {
        let mut finished = Vec::new();
        for (&id, connection) in &mut self.clients {
            let Some(output) = &connection.output else {
                continue;
            };
            match connection.stream.write(&output[connection.written..]) {
                Ok(0) => finished.push(id),
                Ok(written) => {
                    connection.written += written;
                    if connection.written == output.len() {
                        finished.push(id);
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(_) => {
                    self.errors = self.errors.saturating_add(1);
                    finished.push(id)
                }
            }
        }
        for id in finished {
            self.clients.remove(&id);
        }
    }
    pub fn errors(&self) -> u64 {
        self.errors
    }
    pub fn pending(&self) -> bool {
        !self.clients.is_empty()
    }
}
