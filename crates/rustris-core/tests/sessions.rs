#![expect(clippy::unwrap_used, reason = "Explicit fixture assertions")]
mod common;
use common::Fixture;
use rustris_core::{
    Error,
    bytes::OsBytes,
    domain::Runner,
    sessions::{BaseEnvironment, SessionOwner, SessionState},
    store::Store,
};
use std::{
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{PermissionsExt, symlink},
    },
    sync::{Arc, Barrier},
};
use uuid::Uuid;

#[test]
fn populated_wal_schema_one_is_backed_up_before_atomic_upgrade() {
    let fixture = Fixture::new();
    fixture.paths.prepare().unwrap();
    let connection = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    connection
        .execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    connection
        .pragma_update(None, "application_id", 0x5255_5354_i64)
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    fs::set_permissions(fixture.paths.database(), fs::Permissions::from_mode(0o600)).unwrap();
    let game = Uuid::now_v7();
    let installation = Uuid::now_v7();
    let profile = Uuid::now_v7();
    connection
        .execute(
            "INSERT INTO games(id,display_name,favorite) VALUES(?1,'Preserved 🦀',1)",
            [game.as_bytes().as_slice()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO installations(id,game_id) VALUES(?1,?2)",
            rusqlite::params![
                installation.as_bytes().as_slice(),
                game.as_bytes().as_slice()
            ],
        )
        .unwrap();
    connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?1,?2,?3,?4,'native')",rusqlite::params![profile.as_bytes().as_slice(),installation.as_bytes().as_slice(),b"/missing_\xff".as_slice(),b"/".as_slice()]).unwrap();
    let store = Store::open(fixture.paths.clone()).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.entries[0].game_id, game);
    assert!(snapshot.entries[0].favorite);
    assert_eq!(
        snapshot.entries[0].profile.executable.as_bytes(),
        b"/missing_\xff"
    );
    let backups: Vec<_> = fs::read_dir(&fixture.paths.data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-1-backup-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::metadata(&backups[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let backup = rusqlite::Connection::open(&backups[0]).unwrap();
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        backup
            .query_row("SELECT display_name FROM games", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "Preserved 🦀"
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
}

#[test]
fn failed_migration_preserves_populated_original_and_recovery_copy() {
    let fixture = Fixture::new();
    fixture.paths.prepare().unwrap();
    let connection = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    connection
        .execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    connection
        .pragma_update(None, "application_id", 0x5255_5354_i64)
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE launch_sessions(marker TEXT);INSERT INTO launch_sessions VALUES('keep');",
        )
        .unwrap();
    fs::set_permissions(fixture.paths.database(), fs::Permissions::from_mode(0o600)).unwrap();
    let before = fs::read(fixture.paths.database()).unwrap();
    assert!(Store::open(fixture.paths.clone()).is_err());
    assert_eq!(fs::read(fixture.paths.database()).unwrap(), before);
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT marker FROM launch_sessions", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert!(fs::read_dir(&fixture.paths.data).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-1-backup-")
    }));
}

#[test]
fn snapshot_is_exact_immutable_and_independent_of_later_profile_edits() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    let game = store.save(None, &input).unwrap();
    let base = BaseEnvironment::from_pairs(vec![
        (b"PATH".to_vec(), b"/fixture/bin".to_vec()),
        (b"API_TOKEN".to_vec(), b"synthetic-never-capture".to_vec()),
        (b"RUSTRIS_TEST_DIRECTORY".to_vec(), b"/fixture".to_vec()),
    ])
    .unwrap();
    let (session, owner) = store.reserve_session(game, 1, &base).unwrap();
    assert_eq!(session.snapshot.source.profile, input);
    assert!(!format!("{owner:?}").contains(&owner.secret().to_string()));
    assert!(
        session
            .snapshot
            .environment
            .iter()
            .any(|(key, value)| key.as_bytes() == b"PATH" && value.as_bytes() == b"/fixture/bin")
    );
    assert!(
        !session
            .snapshot
            .environment
            .iter()
            .any(|(key, _)| key.as_bytes() == b"API_TOKEN"
                || key.as_bytes() == b"RUSTRIS_TEST_DIRECTORY"
                || key.as_bytes() == b"UNSET")
    );
    assert!(store.save(Some((game, 1)), &input).is_err());
    assert!(store.remove(game).is_err());
    let sql = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    assert!(
        sql.execute(
            "UPDATE launch_sessions SET executable=X'2f78' WHERE id=?1",
            [session.id.as_bytes().as_slice()]
        )
        .is_err()
    );
    assert!(
        sql.execute(
            "DELETE FROM session_arguments WHERE session_id=?1",
            [session.id.as_bytes().as_slice()]
        )
        .is_err()
    );
    assert!(
        sql.execute(
            "INSERT INTO session_environment VALUES(?1,99,X'4b',X'56')",
            [session.id.as_bytes().as_slice()]
        )
        .is_err()
    );
    store
        .transition_session(
            &owner,
            0,
            SessionState::Cancelled,
            "Cancelled before spawn",
            None,
        )
        .unwrap();
    input.name = "New source title".into();
    input.arguments.clear();
    store.save(Some((game, 1)), &input).unwrap();
    drop(store);
    let store = Store::open(fixture.paths.clone()).unwrap();
    let historical = store.session(session.id).unwrap();
    assert_eq!(historical.snapshot.source.profile.name, "Café 🦀");
    assert_eq!(historical.snapshot.source.revision, 1);
    assert_eq!(historical.snapshot.source.profile.arguments.len(), 2);
}

#[test]
fn eight_concurrent_claims_produce_exactly_one_durable_reservation() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let stores: Vec<_> = (0..8)
        .map(|_| Store::open(fixture.paths.clone()).unwrap())
        .collect();
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = stores
        .into_iter()
        .map(|mut storage| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                storage.reserve_session(game, 1, &BaseEnvironment::default())
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::SessionBusy)))
            .count(),
        7
    );
    let sql = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    assert_eq!(
        sql.query_row("SELECT count(*) FROM session_reservations", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn prefix_aliases_share_one_physical_reservation() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let prefix = fixture.root.join("prefix");
    fs::create_dir(&prefix).unwrap();
    let alias = fixture.root.join("alias");
    symlink(&prefix, &alias).unwrap();
    let mut input = fixture.input();
    input.runner = Runner::Umu;
    input.prefix = Some(OsBytes::new(prefix.as_os_str().as_bytes()).unwrap());
    let first = store.save(None, &input).unwrap();
    input.name = "Alias".into();
    input.prefix = Some(OsBytes::new(alias.as_os_str().as_bytes()).unwrap());
    let second = store.save(None, &input).unwrap();
    store
        .reserve_session(first, 1, &BaseEnvironment::default())
        .unwrap();
    assert!(matches!(
        store.reserve_session(second, 1, &BaseEnvironment::default()),
        Err(Error::SessionBusy)
    ));
}

#[test]
fn unknown_requires_warned_release_and_never_authorizes_stale_updates() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let (session, owner) = store
        .reserve_session(game, 1, &BaseEnvironment::default())
        .unwrap();
    let wrong = SessionOwner::from_parts(session.id, Uuid::new_v4());
    assert!(
        store
            .transition_session(&wrong, 0, SessionState::Running, "", None)
            .is_err()
    );
    store
        .transition_session(
            &owner,
            0,
            SessionState::Running,
            "Owned fixture observed",
            None,
        )
        .unwrap();
    let unknown = store
        .mark_session_unknown(session.id, 1, "Helper identity unavailable")
        .unwrap();
    assert_eq!(unknown.state, SessionState::Unknown);
    assert!(matches!(
        store.reserve_session(game, 1, &BaseEnvironment::default()),
        Err(Error::SessionBusy)
    ));
    assert!(store.release_unknown_session(session.id, 2, false).is_err());
    let released = store.release_unknown_session(session.id, 2, true).unwrap();
    assert_eq!(released.state, SessionState::ReleasedUnknown);
    assert!(released.exit_code.is_none());
    store
        .reserve_session(game, 1, &BaseEnvironment::default())
        .unwrap();
    assert!(
        store
            .transition_session(&owner, 3, SessionState::Running, "Stale helper", None)
            .is_err()
    );
}

#[test]
fn failed_terminal_commit_retains_reservation_and_last_observed_state() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let (session, owner) = store
        .reserve_session(game, 1, &BaseEnvironment::default())
        .unwrap();
    store
        .transition_session(&owner, 0, SessionState::Running, "", None)
        .unwrap();
    store
        .transition_session(&owner, 1, SessionState::Finalizing, "", Some(7))
        .unwrap();
    let sql = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_release BEFORE DELETE ON session_reservations BEGIN SELECT RAISE(ABORT,'fixture failure');END;").unwrap();
    assert!(
        store
            .transition_session(&owner, 2, SessionState::Exited, "", Some(7))
            .is_err()
    );
    assert_eq!(
        store.session(session.id).unwrap().state,
        SessionState::Finalizing
    );
    assert!(matches!(
        store.reserve_session(game, 1, &BaseEnvironment::default()),
        Err(Error::SessionBusy)
    ));
    sql.execute_batch("DROP TRIGGER reject_release").unwrap();
    let ended = store
        .transition_session(
            &owner,
            2,
            SessionState::Exited,
            "Observed exit; game health unknown",
            Some(7),
        )
        .unwrap();
    assert_eq!(ended.exit_code, Some(7));
    store
        .reserve_session(game, 1, &BaseEnvironment::default())
        .unwrap();
}

#[test]
fn failed_reservation_rolls_back_every_snapshot_row() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let sql = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_claim BEFORE INSERT ON session_reservations BEGIN SELECT RAISE(ABORT,'fixture claim failure');END;").unwrap();
    assert!(
        store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .is_err()
    );
    for table in [
        "launch_sessions",
        "session_arguments",
        "session_environment",
        "session_environment_delta",
        "session_events",
        "session_reservations",
    ] {
        assert_eq!(
            sql.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[test]
fn credential_capture_and_invalid_transitions_fail_closed() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    input.environment.push(rustris_core::domain::EnvDelta {
        key: OsBytes::new(b"API_TOKEN").unwrap(),
        value: Some(OsBytes::new(b"synthetic-not-real").unwrap()),
    });
    let game = store.save(None, &input).unwrap();
    assert!(
        store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .is_err()
    );
    input.environment.pop();
    store.save(Some((game, 1)), &input).unwrap();
    let base = BaseEnvironment::from_pairs(vec![
        (b"LC_SECRET".to_vec(), b"synthetic-not-real".to_vec()),
        (b"LC_ALL".to_vec(), b"C".to_vec()),
    ])
    .unwrap();
    let (session, owner) = store.reserve_session(game, 2, &base).unwrap();
    assert!(
        !session
            .snapshot
            .environment
            .iter()
            .any(|(key, _)| key.as_bytes() == b"LC_SECRET")
    );
    assert!(
        store
            .transition_session(&owner, 0, SessionState::Exited, "", Some(0))
            .is_err()
    );
    assert!(
        store
            .transition_session(&owner, 0, SessionState::ForceRequested, "", None)
            .is_err()
    );
    assert!(matches!(
        store.transition_session(&owner, 99, SessionState::Running, "", None),
        Err(Error::SessionChanged)
    ));
    assert_eq!(store.session(session.id).unwrap().generation, 0);
}

#[test]
fn missing_active_reservation_is_refused_without_automatic_repair() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let (session, _) = store
        .reserve_session(game, 1, &BaseEnvironment::default())
        .unwrap();
    drop(store);
    let sql = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    sql.execute(
        "DELETE FROM session_reservations WHERE session_id=?1",
        [session.id.as_bytes().as_slice()],
    )
    .unwrap();
    drop(sql);
    let before = fs::read(fixture.paths.database()).unwrap();
    assert!(Store::open(fixture.paths.clone()).is_err());
    assert_eq!(fs::read(fixture.paths.database()).unwrap(), before);
}

#[test]
fn background_open_never_recreates_a_missing_library() {
    let fixture = Fixture::new();
    assert!(Store::open_existing(fixture.paths.clone()).is_err());
    assert!(!fixture.paths.data.exists());
    let store = Store::open(fixture.paths.clone()).unwrap();
    drop(store);
    fs::remove_dir_all(fixture.root.join("owned")).unwrap();
    assert!(Store::open_existing(fixture.paths.clone()).is_err());
    assert!(!fixture.root.join("owned").exists());
}
#[test]
fn active_and_unknown_sessions_share_a_bounded_preparation_budget() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut owners = Vec::new();
    for index in 0..8 {
        let mut input = fixture.input();
        input.name = format!("Fixture {index}");
        let game = store.save(None, &input).unwrap();
        let (_, owner) = store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .unwrap();
        owners.push(owner);
    }
    let mut input = fixture.input();
    input.name = "Ninth fixture".into();
    let game = store.save(None, &input).unwrap();
    assert!(
        store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .is_err()
    );
    store
        .transition_session(
            &owners[0],
            0,
            SessionState::Cancelled,
            "No fixture was spawned",
            None,
        )
        .unwrap();
    assert!(
        store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .is_ok()
    );
}
