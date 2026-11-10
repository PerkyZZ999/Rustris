use super::*;
use native::{CompletionEvidence, LogRecord, LogStats, NativeStatus, Preflight};
use std::fmt::Write;

const TEXT_LIMIT: usize = 128 * 1024;
#[derive(Default)]
pub(super) struct Tail {
    session: Option<Uuid>,
    cursor: u64,
    text: String,
    clipped: bool,
    gap: bool,
    completed: bool,
    history_error: Option<String>,
    last_stats: LogStats,
    history: Option<native::CompletedLog>,
}
impl Tail {
    pub(super) fn reload(&mut self) {
        self.completed = false;
        self.history_error = None;
    }
    fn ingest(&mut self, records: &[LogRecord], gap: bool) {
        self.gap |= gap;
        for record in records
            .iter()
            .filter(|r| r.sequence > self.cursor)
            .collect::<Vec<_>>()
        {
            self.cursor = record.sequence;
            let _ = writeln!(
                self.text,
                "[{} µs · {} · byte {}]",
                record.timestamp_us,
                if record.stream == 1 {
                    "stdout"
                } else {
                    "stderr"
                },
                record.offset
            );
            // Render control bytes literally; text is display-only and never a command.
            for character in String::from_utf8_lossy(&record.data).chars() {
                if character.is_control() && character != '\n' && character != '\t' {
                    let _ = write!(self.text, "\\u{{{:x}}}", character as u32);
                } else {
                    self.text.push(character);
                }
            }
            self.text.push('\n');
        }
        if self.text.len() > TEXT_LIMIT {
            let mut start = self.text.len() - TEXT_LIMIT;
            while !self.text.is_char_boundary(start) {
                start += 1;
            }
            self.text.drain(..start);
            self.clipped = true;
        }
    }
}
#[derive(Serialize)]
pub struct Details {
    pub id: Uuid,
    pub game: Uuid,
    pub name: String,
    pub revision: i64,
    pub executable: String,
    pub cwd: String,
    pub environment_policy: String,
    pub state: String,
    pub generation: i64,
    pub reserved: bool,
    pub can_stop: bool,
    pub can_force: bool,
    pub can_release: bool,
    pub error: Option<String>,
    pub preflight: Option<Preflight>,
    pub root_exit_code: Option<i32>,
    pub root_signal: Option<i32>,
    pub owned_children: Option<usize>,
    pub capture_current: bool,
    pub logs: LogStats,
    pub log_text: String,
    pub presentation_clipped: bool,
    pub log_gap: bool,
    pub log_coverage: native::LogCoverage,
    pub game_health: &'static str,
}
impl Service {
    pub(super) fn details(&mut self, id: Uuid) -> Result<Details> {
        if self.tail.session != Some(id) {
            self.tail = Tail {
                session: Some(id),
                ..Tail::default()
            };
        }
        let mut record = self.storage()?.session(id)?;
        let mut status = NativeStatus::default();
        let mut error = None;
        let mut capture_current = false;
        let mut children = None;
        if !record.state.observed_terminal() && record.state != SessionState::ReleasedUnknown {
            let result = self.client(id).and_then(|client| {
                client.request(Operation::Logs {
                    after: self.tail.cursor,
                })
            });
            match result {
                Ok(reply) if reply.accepted => {
                    if let Some(live) = reply.status {
                        self.tail.ingest(&live.records, live.log_gap);
                        capture_current = true;
                        children = Some(live.owned_children);
                        self.tail.last_stats = live.logs.clone();
                        status = live;
                    }
                }
                Ok(reply) => error = reply.reason,
                Err(e) => {
                    self.clients.remove(&id);
                    error = Some(e.to_string());
                    // Reconnection records unknown only after original identity/peer
                    // verification fails. Never infer completion from a dropped socket.
                }
            }
            record = self.storage()?.session(id)?;
        }
        if record.state.observed_terminal() || record.state == SessionState::ReleasedUnknown {
            if let Ok(completion) = serde_json::from_str::<CompletionEvidence>(&record.detail) {
                status.root_exit_code = completion.root_exit_code;
                status.root_signal = completion.root_signal;
                status.logs = completion.logs;
                capture_current = true;
                children = Some(0);
            } else {
                status.root_exit_code = record.exit_code;
                if matches!(
                    record.state,
                    SessionState::Cancelled | SessionState::LaunchFailed
                ) {
                    capture_current = true;
                    children = Some(0);
                }
            }
            if !self.tail.completed {
                let options = LaunchOptions::discover(self.helper.clone());
                match options.and_then(|o| native::read_completed(&o.state.join("logs"), id)) {
                    Ok(history) => {
                        self.tail.text.clear();
                        self.tail.cursor = 0;
                        self.tail.ingest(
                            &history.records,
                            history.records.first().is_some_and(|r| r.sequence > 1),
                        );
                        self.tail.history = Some(history);
                    }
                    Err(e) => {
                        if status.logs.stdout_read + status.logs.stderr_read > 0
                            || !matches!(
                                record.state,
                                SessionState::Cancelled | SessionState::LaunchFailed
                            )
                        {
                            self.tail.history_error = Some(e.to_string());
                        }
                    }
                }
                self.tail.completed = true;
            }
        }
        if !capture_current {
            status.logs = self.tail.last_stats.clone();
            status.logs.capture_unavailable = true;
        }
        let reserved =
            !record.state.observed_terminal() && record.state != SessionState::ReleasedUnknown;
        let preflight = self.storage()?.native_preflight(id)?;
        Ok(Details {
            id,
            game: record.snapshot.source.game_id,
            name: record.snapshot.source.profile.name,
            revision: record.snapshot.source.revision,
            executable: record.snapshot.source.profile.executable.label(),
            cwd: record.snapshot.source.profile.cwd.label(),
            environment_policy: record.snapshot.environment_policy,
            state: record.state.key().into(),
            generation: record.generation,
            reserved,
            can_stop: reserved && status.can_stop,
            can_force: reserved && status.can_force,
            can_release: record.state == SessionState::Unknown,
            error: error.or(status.error).or(self.tail.history_error.clone()),
            preflight,
            root_exit_code: status.root_exit_code,
            root_signal: status.root_signal,
            owned_children: children,
            capture_current,
            logs: status.logs,
            log_text: self.tail.text.clone(),
            presentation_clipped: self.tail.clipped,
            log_gap: self.tail.gap,
            log_coverage: native::LogCoverage::for_runner(record.snapshot.source.profile.runner),
            game_health: "unknown",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_tail_bounds_unicode_and_escapes_untrusted_control_characters() {
        let mut tail = Tail::default();
        let data = "🦀\u{1b}[31m\0\r\n".repeat(32768).into_bytes();
        tail.ingest(
            &[LogRecord {
                sequence: 1,
                stream: 2,
                timestamp_us: 12,
                offset: 0,
                data,
            }],
            false,
        );
        assert!(tail.clipped);
        assert!(tail.text.len() <= TEXT_LIMIT);
        assert!(tail.text.contains("\\u{1b}"));
        assert!(!tail.text.contains('\u{1b}'));
        assert!(!tail.text.contains('\0'));
        assert!(tail.text.is_char_boundary(0));
        let before = tail.text.clone();
        tail.ingest(
            &[LogRecord {
                sequence: 1,
                stream: 1,
                timestamp_us: 0,
                offset: 0,
                data: b"duplicate".to_vec(),
            }],
            true,
        );
        assert_eq!(tail.text, before);
        assert!(tail.gap);
    }
}
