use super::*;
use crate::{
    domain::{EnvDelta, ProfileInput, Runner},
    store::Store,
};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

impl Store {
    pub(crate) fn session_head(&self, session: Uuid) -> Result<(SessionState, i64)> {
        let (state, generation): (String, i64) = self.connection.query_row(
            "SELECT state,generation FROM launch_sessions WHERE id=?1 AND sealed=1",
            [session.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((SessionState::parse(&state)?, generation))
    }
    pub(crate) fn session_capability(&self, session: Uuid) -> Result<SessionOwner> {
        let secret: Vec<u8> = self.connection.query_row(
            "SELECT secret FROM launch_sessions WHERE id=?1",
            [session.as_bytes().as_slice()],
            |row| row.get(0),
        )?;
        Ok(SessionOwner {
            id: session,
            secret: id(secret)?,
        })
    }
    pub(crate) fn register_helper(
        &mut self,
        owner: &SessionOwner,
        generation: i64,
        helper: &HelperIdentity,
    ) -> Result<Session> {
        if helper.pid == 0
            || helper.start_ticks == 0
            || !helper.socket.path().is_absolute()
            || helper.socket.as_bytes().len() >= 108
        {
            return Err(Error::Invalid(
                "The helper identity/socket is invalid.".into(),
            ));
        }
        let ticks = i64::try_from(helper.start_ticks).map_err(|_| {
            Error::Invalid("The helper start time exceeds the supported range.".into())
        })?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if authorized(&transaction, owner, generation)? != SessionState::Preparing {
            return Err(Error::SessionChanged);
        }
        let changed=transaction.execute("UPDATE launch_sessions SET helper_pid=?1,helper_start_ticks=?2,helper_boot_id=?3,socket=?4 WHERE id=?5 AND helper_pid IS NULL",params![helper.pid,ticks,helper.boot_id.as_bytes().as_slice(),helper.socket.as_bytes(),owner.id.as_bytes().as_slice()])?;
        if changed != 1 {
            return Err(Error::Invalid(
                "A helper is already registered; reconcile the original session.".into(),
            ));
        }
        write_transition(
            &transaction,
            owner.id,
            generation,
            SessionState::Preparing,
            "Native helper identity registered; no game has started.",
            None,
        )?;
        transaction.commit()?;
        self.session(owner.id)
            .map_err(|error| Error::CommittedRefresh(Box::new(error)))
    }
    pub fn record_preflight(
        &mut self,
        owner: &SessionOwner,
        generation: i64,
        report: &native::Preflight,
    ) -> Result<Session> {
        report.validate()?;
        let detail = serde_json::to_string(report).map_err(|e| Error::Invalid(e.to_string()))?;
        if detail.len() > 32768 {
            return Err(Error::Invalid("Preflight evidence exceeds 32 KiB.".into()));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if authorized(&transaction, owner, generation)? != SessionState::Preparing {
            return Err(Error::SessionChanged);
        }
        write_transition(
            &transaction,
            owner.id,
            generation,
            SessionState::Preparing,
            "Preflight evidence recorded for this preparing generation.",
            None,
        )?;
        transaction.execute(
            "INSERT INTO session_preflights VALUES(?1,?2,?3)",
            params![owner.id.as_bytes().as_slice(), generation + 1, detail],
        )?;
        transaction.commit()?;
        self.session(owner.id)
            .map_err(|error| Error::CommittedRefresh(Box::new(error)))
    }
    pub(crate) fn validate_sessions(&self) -> Result<()> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version >= 3 {
            let invalid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM session_runners p JOIN launch_sessions s ON s.id=p.session_id WHERE s.runner!='umu') OR EXISTS(SELECT 1 FROM session_runners p JOIN session_reservations r ON r.session_id=p.session_id WHERE r.prefix_path IS NULL OR p.prefix_path!=r.prefix_path OR p.prefix_device!=r.prefix_device OR p.prefix_inode!=r.prefix_inode)",[],|r|r.get(0))?;
            if invalid {
                return Err(Error::Database(
                    "A saved runner prefix does not match its reservation".into(),
                ));
            }
        }
        let ids = self
            .connection
            .prepare("SELECT s.id FROM launch_sessions s JOIN session_reservations r ON r.session_id=s.id")?
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for bytes in ids {
            self.session(id(bytes)?)?;
        }
        let invalid:bool=self.connection.query_row("SELECT EXISTS(SELECT 1 FROM session_reservations r JOIN launch_sessions s ON s.id=r.session_id WHERE r.profile_id!=s.profile_id OR s.state IN ('exited','stopped','cancelled','launch_failed','released_unknown')) OR EXISTS(SELECT 1 FROM launch_sessions s LEFT JOIN session_reservations r ON r.session_id=s.id WHERE r.session_id IS NULL AND s.state NOT IN ('exited','stopped','cancelled','launch_failed','released_unknown')) OR EXISTS(SELECT 1 FROM launch_sessions s LEFT JOIN session_events e ON e.session_id=s.id AND e.generation=s.generation WHERE e.session_id IS NULL OR e.state!=s.state)",[],|row|row.get(0))?;
        if invalid {
            return Err(Error::Database(
                "The session ledger has inconsistent state or reservations".into(),
            ));
        }
        Ok(())
    }
    /// Claim a profile and effective prefix atomically with its immutable snapshot.
    /// This records preparation only; it does not establish process/game health.
    pub fn reserve_session(
        &mut self,
        game: Uuid,
        revision: i64,
        base: &BaseEnvironment,
    ) -> Result<(Session, SessionOwner)> {
        self.reserve(game, revision, base, None)
    }
    pub fn reserve_umu_session(
        &mut self,
        game: Uuid,
        revision: i64,
        prepared: &crate::runners::umu::PreparedCommand,
    ) -> Result<(Session, SessionOwner)> {
        self.reserve(game, revision, &BaseEnvironment::default(), Some(prepared))
    }
    fn reserve(
        &mut self,
        game: Uuid,
        revision: i64,
        base: &BaseEnvironment,
        prepared: Option<&crate::runners::umu::PreparedCommand>,
    ) -> Result<(Session, SessionOwner)> {
        let entry = self
            .snapshot()?
            .entries
            .into_iter()
            .find(|entry| entry.game_id == game)
            .ok_or(Error::NotFound)?;
        if entry.revision != revision {
            return Err(Error::Conflict);
        }
        if let Some(prepared) = prepared {
            prepared.validate_source(&entry.profile)?;
        }
        let environment = if let Some(prepared) = prepared {
            prepared.environment.clone()
        } else {
            base.effective(&entry.profile)?
        };
        let prefix = if entry.profile.runner == Runner::Umu {
            let raw = entry
                .profile
                .prefix
                .as_ref()
                .ok_or_else(|| Error::Invalid("The Wine prefix is missing.".into()))?;
            let identity = crate::runners::umu::existing_prefix(&raw.path())?;
            Some((
                identity.path,
                identity.device.to_be_bytes(),
                identity.inode.to_be_bytes(),
            ))
        } else {
            None
        };
        if let (Some(prepared), Some((path, device, inode))) = (prepared, prefix.as_ref())
            && (*path != prepared.prefix.path
                || *device != prepared.prefix.device.to_be_bytes()
                || *inode != prepared.prefix.inode.to_be_bytes())
        {
            return Err(Error::Invalid(
                "The prepared prefix changed before reservation. Prepare it again.".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current:Option<(Vec<u8>,i64)>=transaction.query_row("SELECT p.id,p.revision FROM profiles p JOIN installations i ON p.installation_id=i.id WHERE i.game_id=?1",[game.as_bytes().as_slice()],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
        if current != Some((entry.profile_id.as_bytes().to_vec(), revision)) {
            return Err(Error::Conflict);
        }
        let conflict:bool=transaction.query_row("SELECT EXISTS(SELECT 1 FROM session_reservations WHERE profile_id=?1 OR prefix_path=?2 OR (prefix_device=?3 AND prefix_inode=?4))",params![entry.profile_id.as_bytes().as_slice(),prefix.as_ref().map(|p|p.0.as_bytes()),prefix.as_ref().map(|p|p.1.as_slice()),prefix.as_ref().map(|p|p.2.as_slice())],|row|row.get(0))?;
        if conflict {
            return Err(Error::SessionBusy);
        }
        let active: i64 =
            transaction.query_row("SELECT count(*) FROM session_reservations", [], |row| {
                row.get(0)
            })?;
        if active >= 8 {
            return Err(Error::Invalid("Eight sessions are active or unknown. Finish or reconcile one before launching another.".into()));
        }
        let owner = SessionOwner {
            id: Uuid::now_v7(),
            secret: Uuid::new_v4(),
        };
        let session = owner.id.as_bytes().as_slice();
        let created = now_ms()?;
        transaction.execute("INSERT INTO launch_sessions(id,game_id,installation_id,profile_id,revision,display_name,runner,executable,cwd,prefix,environment_policy,state,created_ms,secret) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'preparing',?12,?13)",params![session,game.as_bytes().as_slice(),entry.installation_id.as_bytes().as_slice(),entry.profile_id.as_bytes().as_slice(),revision,entry.profile.name,entry.profile.runner.key(),entry.profile.executable.as_bytes(),entry.profile.cwd.as_bytes(),entry.profile.prefix.as_ref().map(OsBytes::as_bytes),ENVIRONMENT_POLICY,created,owner.secret.as_bytes().as_slice()])?;
        for (position, value) in entry.profile.arguments.iter().enumerate() {
            transaction.execute(
                "INSERT INTO session_arguments VALUES(?1,?2,?3)",
                params![session, position as i64, value.as_bytes()],
            )?;
        }
        for (position, delta) in entry.profile.environment.iter().enumerate() {
            transaction.execute(
                "INSERT INTO session_environment_delta VALUES(?1,?2,?3,?4)",
                params![
                    session,
                    position as i64,
                    delta.key.as_bytes(),
                    delta.value.as_ref().map(OsBytes::as_bytes)
                ],
            )?;
        }
        for (position, (key, value)) in environment.iter().enumerate() {
            transaction.execute(
                "INSERT INTO session_environment VALUES(?1,?2,?3,?4)",
                params![session, position as i64, key.as_bytes(), value.as_bytes()],
            )?;
        }
        let runtime = prepared
            .map(|command| super::runner_storage::insert(&transaction, owner.id, command))
            .transpose()?;
        transaction.execute("UPDATE launch_sessions SET sealed=1 WHERE id=?1", [session])?;
        transaction.execute(
            "INSERT INTO session_reservations VALUES(?1,?2,?3,?4,?5)",
            params![
                session,
                entry.profile_id.as_bytes().as_slice(),
                prefix.as_ref().map(|p| p.0.as_bytes()),
                prefix.as_ref().map(|p| p.1.as_slice()),
                prefix.as_ref().map(|p| p.2.as_slice())
            ],
        )?;
        transaction.execute(
            "INSERT INTO session_events VALUES(?1,0,?2,'preparing','')",
            params![session, created],
        )?;
        transaction.commit()?;
        Ok((
            Session {
                id: owner.id,
                snapshot: LaunchSnapshot {
                    source: entry,
                    environment,
                    environment_policy: ENVIRONMENT_POLICY.into(),
                    runtime,
                },
                state: SessionState::Preparing,
                generation: 0,
                detail: String::new(),
                exit_code: None,
                helper: None,
            },
            owner,
        ))
    }

    /// Load history without consulting mutable game files or the edited source profile.
    pub fn session(&self, session: Uuid) -> Result<Session> {
        let transaction = self.connection.unchecked_transaction()?;
        let mut statement=transaction.prepare("SELECT game_id,installation_id,profile_id,revision,display_name,runner,executable,cwd,prefix,environment_policy,state,generation,detail,exit_code,sealed,helper_pid,helper_start_ticks,helper_boot_id,socket FROM launch_sessions WHERE id=?1")?;
        let mut rows = statement.query([session.as_bytes().as_slice()])?;
        let row = rows.next()?.ok_or(Error::NotFound)?;
        if row.get::<_, i64>(14)? != 1 {
            return Err(Error::Database(
                "An incomplete session snapshot was found".into(),
            ));
        }
        let arguments=transaction.prepare("SELECT value FROM session_arguments WHERE session_id=?1 ORDER BY position LIMIT 1025")?.query_map([session.as_bytes().as_slice()],|r|r.get::<_,Vec<u8>>(0))?.map(|r|OsBytes::new(r?)).collect::<Result<Vec<_>>>()?;
        let deltas=transaction.prepare("SELECT key,value FROM session_environment_delta WHERE session_id=?1 ORDER BY position LIMIT 257")?.query_map([session.as_bytes().as_slice()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Option<Vec<u8>>>(1)?)))?.map(|r|{let(key,value)=r?;Ok(EnvDelta{key:OsBytes::new(key)?,value:value.map(OsBytes::new).transpose()?})}).collect::<Result<Vec<_>>>()?;
        let environment=transaction.prepare("SELECT key,value FROM session_environment WHERE session_id=?1 ORDER BY position LIMIT 257")?.query_map([session.as_bytes().as_slice()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?)))?.map(|r|{let(key,value)=r?;Ok((OsBytes::new(key)?,OsBytes::new(value)?))}).collect::<Result<Vec<_>>>()?;
        let profile = ProfileInput {
            name: row.get(4)?,
            executable: OsBytes::new(row.get::<_, Vec<u8>>(6)?)?,
            cwd: OsBytes::new(row.get::<_, Vec<u8>>(7)?)?,
            arguments,
            environment: deltas,
            runner: Runner::parse(&row.get::<_, String>(5)?)?,
            prefix: row
                .get::<_, Option<Vec<u8>>>(8)?
                .map(OsBytes::new)
                .transpose()?,
        };
        profile.validate()?;
        let mut effective_profile = profile.clone();
        effective_profile.environment = environment
            .iter()
            .map(|(key, value)| EnvDelta {
                key: key.clone(),
                value: Some(value.clone()),
            })
            .collect();
        effective_profile.validate()?;
        let helper = match row.get::<_, Option<u32>>(15)? {
            Some(pid) => Some(HelperIdentity {
                pid,
                start_ticks: u64::try_from(row.get::<_, i64>(16)?)
                    .map_err(|_| Error::Database("A helper start time is invalid".into()))?,
                boot_id: id(row.get(17)?)?,
                socket: OsBytes::new(row.get::<_, Vec<u8>>(18)?)?,
            }),
            None => None,
        };
        let runtime = super::runner_storage::load(&transaction, session)?;
        if runtime.is_some() && profile.runner != Runner::Umu {
            return Err(Error::Database(
                "A native session has unsupported runner metadata".into(),
            ));
        }
        let output = Session {
            id: session,
            snapshot: LaunchSnapshot {
                source: Entry {
                    game_id: id(row.get(0)?)?,
                    installation_id: id(row.get(1)?)?,
                    profile_id: id(row.get(2)?)?,
                    revision: row.get(3)?,
                    favorite: false,
                    artwork: None,
                    artwork_available: false,
                    review_approved: false,
                    fetched_cover: None,
                    fetched_hero: None,
                    profile,
                    availability: "Historical launch configuration".into(),
                },
                environment,
                environment_policy: row.get(9)?,
                runtime,
            },
            state: SessionState::parse(&row.get::<_, String>(10)?)?,
            generation: row.get(11)?,
            detail: row.get(12)?,
            exit_code: row.get(13)?,
            helper,
        };
        drop(rows);
        drop(statement);
        transaction.commit()?;
        Ok(output)
    }

    /// Authenticated helper state changes retain reservations on every failed write.
    pub fn transition_session(
        &mut self,
        owner: &SessionOwner,
        generation: i64,
        next: SessionState,
        detail: &str,
        exit_code: Option<i32>,
    ) -> Result<Session> {
        valid_detail(detail)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = authorized(&transaction, owner, generation)?;
        if !current.permits(next) {
            return Err(Error::Invalid(
                "That session transition is not permitted.".into(),
            ));
        }
        write_transition(&transaction, owner.id, generation, next, detail, exit_code)?;
        transaction.commit()?;
        self.session(owner.id)
            .map_err(|error| Error::CommittedRefresh(Box::new(error)))
    }
    /// Controller-side loss of evidence is conservative and never releases a reservation.
    pub fn mark_session_unknown(
        &mut self,
        session: Uuid,
        generation: i64,
        detail: &str,
    ) -> Result<Session> {
        valid_detail(detail)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (state, actual): (String, i64) = transaction.query_row(
            "SELECT state,generation FROM launch_sessions WHERE id=?1",
            [session.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if actual != generation {
            return Err(Error::SessionChanged);
        }
        let state = SessionState::parse(&state)?;
        if state.observed_terminal() || state == SessionState::ReleasedUnknown {
            return Err(Error::Invalid(
                "A completed session cannot become unknown.".into(),
            ));
        }
        write_transition(
            &transaction,
            session,
            generation,
            SessionState::Unknown,
            detail,
            None,
        )?;
        transaction.commit()?;
        self.session(session)
            .map_err(|error| Error::CommittedRefresh(Box::new(error)))
    }
    /// Explicitly warned release permits another launch, without claiming the game ended.
    pub fn release_unknown_session(
        &mut self,
        session: Uuid,
        generation: i64,
        acknowledged: bool,
    ) -> Result<Session> {
        if !acknowledged {
            return Err(Error::Invalid("The game may still be running. Explicitly acknowledge concurrent-launch risk before releasing this reservation.".into()));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (state, actual): (String, i64) = transaction.query_row(
            "SELECT state,generation FROM launch_sessions WHERE id=?1",
            [session.as_bytes().as_slice()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if actual != generation {
            return Err(Error::SessionChanged);
        }
        if SessionState::parse(&state)? != SessionState::Unknown {
            return Err(Error::Invalid(
                "Only an unknown session can be explicitly released.".into(),
            ));
        }
        write_transition(
            &transaction,
            session,
            generation,
            SessionState::ReleasedUnknown,
            "Reservation released with concurrent-launch risk acknowledged; process outcome remains unknown.",
            None,
        )?;
        transaction.execute(
            "DELETE FROM session_reservations WHERE session_id=?1",
            [session.as_bytes().as_slice()],
        )?;
        transaction.commit()?;
        self.session(session)
            .map_err(|error| Error::CommittedRefresh(Box::new(error)))
    }
}
fn authorized(
    transaction: &rusqlite::Transaction<'_>,
    owner: &SessionOwner,
    generation: i64,
) -> Result<SessionState> {
    let row: Option<(String, i64)> = transaction
        .query_row(
            "SELECT state,generation FROM launch_sessions WHERE id=?1 AND secret=?2",
            params![
                owner.id.as_bytes().as_slice(),
                owner.secret.as_bytes().as_slice()
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (state, actual) =
        row.ok_or_else(|| Error::Invalid("The session handshake was not accepted.".into()))?;
    if actual != generation {
        return Err(Error::SessionChanged);
    }
    SessionState::parse(&state)
}
fn write_transition(
    transaction: &rusqlite::Transaction<'_>,
    session: Uuid,
    generation: i64,
    next: SessionState,
    detail: &str,
    exit_code: Option<i32>,
) -> Result<()> {
    if exit_code.is_some()
        && !matches!(
            next,
            SessionState::Exited | SessionState::Stopped | SessionState::Finalizing
        )
    {
        return Err(Error::Invalid(
            "Exit evidence is invalid for this transition.".into(),
        ));
    }
    let now = now_ms()?;
    transaction.execute("UPDATE launch_sessions SET state=?1,generation=generation+1,detail=?2,exit_code=COALESCE(?3,exit_code),completed_ms=?4 WHERE id=?5 AND generation=?6",params![next.key(),detail,exit_code,next.observed_terminal().then_some(now),session.as_bytes().as_slice(),generation])?;
    transaction.execute(
        "INSERT INTO session_events VALUES(?1,?2,?3,?4,?5)",
        params![
            session.as_bytes().as_slice(),
            generation + 1,
            now,
            next.key(),
            detail
        ],
    )?;
    if next.observed_terminal() {
        transaction.execute(
            "DELETE FROM session_reservations WHERE session_id=?1",
            [session.as_bytes().as_slice()],
        )?;
    }
    Ok(())
}
