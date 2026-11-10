use super::*;
use crate::sessions::native::protocol::{Operation, Reply, VERSION};

impl Supervisor {
    pub(super) fn handle(&mut self, operation: Operation) -> Reply {
        if let Err(error) = self.synchronize_record() {
            self.lose_tracking(error);
        }
        let after = match &operation {
            Operation::Logs { after } => Some(*after),
            _ => None,
        };
        let result = match operation {
            Operation::Status | Operation::Logs { .. } => Ok(()),
            Operation::Start => self.start_request(),
            Operation::Cancel if !self.spawned && self.state == SessionState::Preparing => self
                .persist(
                    SessionState::Cancelled,
                    "Cancelled before any native process was spawned.",
                    None,
                ),
            Operation::Cancel => Err(Error::Invalid(
                "Preparation has already spawned; use the separate Stop action.".into(),
            )),
            Operation::Stop => self.request_stop(false),
            Operation::Force => self.request_stop(true),
            Operation::Detach => self.detach(),
            Operation::Attach => self.attach(),
        };
        Reply {
            version: VERSION,
            session: self.record.id,
            accepted: result.is_ok(),
            reason: result.err().map(|error| error.to_string()),
            status: Some(self.status(after)),
        }
    }
    fn synchronize_record(&mut self) -> Result<()> {
        let (state, generation) = self.store.session_head(self.record.id)?;
        if generation == self.record.generation {
            return Ok(());
        }
        self.record.state = state;
        self.record.generation = generation;
        if state == SessionState::Unknown && !self.unknown {
            self.detached = true;
            return Ok(());
        }
        self.unknown = true;
        self.state = SessionState::Unknown;
        self.error=Some("The durable session changed outside the current helper operation; control is unavailable.".into());
        Ok(())
    }
    fn start_request(&mut self) -> Result<()> {
        if self.spawned || self.state != SessionState::Preparing {
            return Err(Error::Invalid(
                "This session cannot start another process.".into(),
            ));
        }
        if let Err(error) = self.start() {
            if self.spawned {
                self.lose_tracking(&error);
            } else {
                self.error = Some(error.to_string());
                if let Err(storage) = self.persist(
                    SessionState::LaunchFailed,
                    "Native preparation failed before any process was spawned.",
                    None,
                ) {
                    self.error = Some(storage.to_string());
                }
            }
            return Err(error);
        }
        Ok(())
    }
    fn request_stop(&mut self, force: bool) -> Result<()> {
        if !self.spawned || self.unknown || self.detached || self.children.is_empty() {
            return Err(Error::Invalid(
                "Current attached ownership is unavailable; no signal was sent.".into(),
            ));
        }
        if !force && self.signal == Some(Signal::TERM) || self.signal == Some(Signal::KILL) {
            return Ok(());
        }
        if force && self.signal != Some(Signal::TERM) {
            return Err(Error::Invalid(
                "Request graceful stop before explicitly escalating to force.".into(),
            ));
        }
        let (state, message, signal) = if force {
            (
                SessionState::ForceRequested,
                "Explicit force stop requested for the same verified scope.",
                Signal::KILL,
            )
        } else {
            (
                SessionState::StopRequested,
                "Explicit graceful stop requested for verified owned children.",
                Signal::TERM,
            )
        };
        self.persist(state, message, None)?;
        self.signal = Some(signal);
        Ok(())
    }
    fn detach(&mut self) -> Result<()> {
        if self.detached {
            return Ok(());
        }
        if !self.spawned || self.unknown {
            return Err(Error::Invalid(
                "Only an observed spawned session can be detached.".into(),
            ));
        }
        let record = self.store.mark_session_unknown(
            self.record.id,
            self.record.generation,
            "The UI left; the original helper continues observing and draining output.",
        )?;
        self.record = record;
        self.detached = true;
        Ok(())
    }
    fn attach(&mut self) -> Result<()> {
        if !self.spawned {
            return Err(Error::Invalid(
                "No spawned process is available to reattach.".into(),
            ));
        }
        if !self.detached && !self.unknown {
            return Ok(());
        }
        // Fresh waitable pidfds and the full current child list establish scope;
        // persisted PIDs and a healthy-looking old status do not.
        self.observe()
            .map_err(|e| crate::error::io("reverify native ownership", e))?;
        self.store.validate_sessions()?;
        let reserved:bool=self.store.connection.query_row("SELECT EXISTS(SELECT 1 FROM session_reservations WHERE session_id=?1 AND profile_id=?2)",rusqlite::params![self.record.id.as_bytes().as_slice(),self.record.snapshot.source.profile_id.as_bytes().as_slice()],|row|row.get(0))?;
        if !reserved {
            return Err(Error::Invalid(
                "The original reservation was released; this helper cannot resume control.".into(),
            ));
        }
        let (state, generation) = self.store.session_head(self.record.id)?;
        self.record.state = state;
        self.record.generation = generation;
        if state != SessionState::Unknown {
            self.record = self.store.mark_session_unknown(
                self.record.id,
                generation,
                "Rechecking the original helper's live ownership before resuming observation.",
            )?;
        }
        let previous = if self.children.is_empty() {
            SessionState::Finalizing
        } else if self.signal == Some(Signal::KILL) {
            SessionState::ForceRequested
        } else if self.signal == Some(Signal::TERM) {
            SessionState::StopRequested
        } else {
            SessionState::Running
        };
        let resumed = if previous == SessionState::Finalizing {
            SessionState::Finalizing
        } else {
            SessionState::Running
        };
        self.persist(
            resumed,
            "Reattached through fresh owned pidfds and the original private helper identity.",
            if resumed == SessionState::Finalizing {
                self.root_code
            } else {
                None
            },
        )?;
        if matches!(
            previous,
            SessionState::StopRequested | SessionState::ForceRequested
        ) {
            self.persist(
                SessionState::StopRequested,
                "Resumed observation of the previously accepted graceful stop.",
                None,
            )?;
        }
        if previous == SessionState::ForceRequested {
            self.persist(
                SessionState::ForceRequested,
                "Resumed observation of the previously accepted explicit force request.",
                None,
            )?;
        }
        self.unknown = false;
        self.unknown_reported = false;
        self.detached = false;
        self.error = None;
        Ok(())
    }
}
