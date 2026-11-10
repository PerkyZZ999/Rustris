use super::*;

pub(super) fn reader(
    stream: u8,
    received: mpsc::Receiver<Pipe>,
    chunks: SyncSender<Arc<LogRecord>>,
    shared: Arc<Mutex<Capture>>,
    completed: Done,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(format!("rustris-pipe-{stream}"))
        .spawn(move || {
            let _completed = completed;
            let Ok(mut pipe) = received.recv() else {
                return;
            };
            let mut buffer = [0; CHUNK];
            loop {
                let count = match pipe.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => count,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                        continue;
                    }
                    Err(error) => {
                        if let Ok(mut capture) = locked(&shared) {
                            capture.stats.error(format!("Pipe read failed: {error}"));
                        }
                        break;
                    }
                };
                // Sequence assignment and the nonblocking send share one order.
                // No filesystem operation runs under this capture lock.
                match locked(&shared) {
                    Ok(mut capture) => {
                        let record = capture.ingest(stream, buffer[..count].to_vec());
                        match chunks.try_send(record) {
                            Ok(()) => {}
                            Err(TrySendError::Full(record)) => {
                                capture.stats.disk_queue_dropped = capture
                                    .stats
                                    .disk_queue_dropped
                                    .saturating_add(record.data.len() as u64)
                            }
                            Err(TrySendError::Disconnected(record)) => {
                                capture.stats.disk_error_dropped = capture
                                    .stats
                                    .disk_error_dropped
                                    .saturating_add(record.data.len() as u64);
                                capture.stats.error("The log writer is unavailable");
                            }
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error,"Capture unavailable; continuing to drain the pipe")
                    }
                }
            }
        })
}

pub(super) fn writer(
    paths: Paths,
    directory: PathBuf,
    session: Uuid,
    receiver: mpsc::Receiver<Arc<LogRecord>>,
    shared: Arc<Mutex<Capture>>,
    completed: Done,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("rustris-log-writer".into())
        .spawn(move || {
            let _completed = completed;
            let mut sink = match Sink::open(&paths, &directory, session) {
                Ok(sink) => Some(sink),
                Err(error) => {
                    if let Ok(mut capture) = locked(&shared) {
                        capture.stats.error(format!("Log open failed: {error}"));
                    }
                    None
                }
            };
            for record in receiver {
                let result = sink.as_mut().map(|sink| sink.append(&record));
                let physical = if matches!(&result, Some(Err(_))) {
                    sink.as_ref().map(Sink::physical_size)
                } else {
                    None
                };
                if let Ok(mut capture) = locked(&shared) {
                    match result {
                        Some(Ok(written)) => {
                            capture.stats.disk_bytes = written.total;
                            capture.stats.disk_payload =
                                capture.stats.disk_payload.saturating_add(written.payload);
                            capture.stats.disk_limit_dropped = capture
                                .stats
                                .disk_limit_dropped
                                .saturating_add(record.data.len() as u64 - written.payload);
                            capture.stats.evicted_logs =
                                capture.stats.evicted_logs.saturating_add(written.evicted);
                        }
                        Some(Err(error)) => {
                            match physical {
                                Some(Ok(bytes)) => capture.stats.disk_bytes = bytes,
                                Some(Err(_)) | None => {
                                    capture.stats.disk_size_unknown = true;
                                    capture.stats.error("Partial log size is unavailable");
                                }
                            }
                            capture.stats.disk_error_dropped = capture
                                .stats
                                .disk_error_dropped
                                .saturating_add(record.data.len() as u64);
                            capture.stats.error(format!("Log write failed: {error}"));
                            sink = None;
                        }
                        None => {
                            capture.stats.disk_error_dropped = capture
                                .stats
                                .disk_error_dropped
                                .saturating_add(record.data.len() as u64);
                        }
                    }
                }
            }
            if let Some(mut sink) = sink
                && let Err(error) = sink.flush()
                && let Ok(mut capture) = locked(&shared)
            {
                capture.stats.error(format!("Log sync failed: {error}"));
            }
        })
}
