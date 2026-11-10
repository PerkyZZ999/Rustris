use super::{SessionState, id, native::Preflight};
use crate::{Result, store::Store};
use rusqlite::OptionalExtension;
use serde::Serialize;
use uuid::Uuid;

/// Presentation metadata deliberately excludes execution bytes and capabilities.
#[derive(Clone, Debug, Serialize)]
pub struct Overview {
    pub id: Uuid,
    pub game: Uuid,
    pub name: String,
    pub state: String,
    pub generation: i64,
    pub created_ms: i64,
    pub reserved: bool,
}
impl Store {
    /// Eight unresolved sessions plus at most 32 recent sessions of one game.
    pub fn session_overview(&self, game: Option<Uuid>) -> Result<Vec<Overview>> {
        let mut query=self.connection.prepare("SELECT s.id,s.game_id,s.display_name,s.state,s.generation,s.created_ms,EXISTS(SELECT 1 FROM session_reservations r WHERE r.session_id=s.id) FROM launch_sessions s WHERE s.id IN (SELECT session_id FROM session_reservations) OR s.id IN (SELECT id FROM launch_sessions WHERE game_id=?1 ORDER BY created_ms DESC LIMIT 32) ORDER BY created_ms DESC LIMIT 40")?;
        let rows = query.query_map([game.map(|g| g.as_bytes().to_vec())], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, bool>(6)?,
            ))
        })?;
        rows.map(|row| {
            let (session, game, name, state, generation, created_ms, reserved) = row?;
            SessionState::parse(&state)?;
            Ok(Overview {
                id: id(session)?,
                game: id(game)?,
                name,
                state,
                generation,
                created_ms,
                reserved,
            })
        })
        .collect()
    }
    pub fn native_preflight(&self, session: Uuid) -> Result<Option<Preflight>> {
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version >= 3 {
            let detail:Option<String>=self.connection.query_row("SELECT report FROM session_preflights WHERE session_id=?1 ORDER BY generation DESC LIMIT 1",[session.as_bytes().as_slice()],|r|r.get(0)).optional()?;
            if let Some(detail) = detail {
                let report: Preflight = serde_json::from_str(&detail).map_err(|_| {
                    crate::Error::Database("The saved preflight evidence is invalid".into())
                })?;
                report.validate()?;
                return Ok(Some(report));
            }
        }
        let detail:Option<String>=self.connection.query_row("SELECT detail FROM session_events WHERE session_id=?1 AND state='preparing' AND detail LIKE '{\"observations\":%' ORDER BY generation DESC LIMIT 1",[session.as_bytes().as_slice()],|r|r.get(0)).optional()?;
        detail
            .map(|d| {
                serde_json::from_str(&d).map_err(|_| {
                    crate::Error::Database("The saved preflight evidence is invalid".into())
                })
            })
            .transpose()
    }
}
