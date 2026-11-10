#![expect(clippy::unwrap_used, reason = "Private populated migration assertions")]
mod common;
use common::Fixture;
use rustris_core::{
    Error,
    sessions::{
        SessionState,
        native::{Finding, Observation, Preflight},
    },
    store::Store,
};
use std::{fs, os::unix::fs::PermissionsExt};
use uuid::Uuid;
#[test]
#[ignore = "Invoked only by the isolated migration interruption fixture"]
fn migration_interruption_child() {
    let root = std::env::var_os("RUSTRIS_MIGRATION_CHILD_ROOT").unwrap();
    let paths = rustris_core::paths::Paths::in_root(&std::path::PathBuf::from(root));
    Store::open(paths).unwrap();
}
#[test]
#[ignore = "Invoked only by the isolated uncommitted DDL fixture"]
fn uncommitted_ddl_child() {
    let root = std::path::PathBuf::from(std::env::var_os("RUSTRIS_DDL_CHILD_ROOT").unwrap());
    let paths = rustris_core::paths::Paths::in_root(&root);
    let mut connection = rusqlite::Connection::open(paths.database()).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    connection.pragma_update(None, "cache_size", 10).unwrap();
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    transaction
        .execute_batch(include_str!("../src/schema004.sql"))
        .unwrap();
    transaction.pragma_update(None, "user_version", 4).unwrap();
    transaction.execute_batch("CREATE TABLE interruption_padding(data BLOB); INSERT INTO interruption_padding VALUES(zeroblob(8388608));").unwrap();
    assert!(!transaction.is_autocommit());
    fs::write(root.join("ddl-ready"), b"uncommitted").unwrap();
    std::thread::sleep(std::time::Duration::from_secs(30));
}
#[test]
fn interrupted_backup_leaves_original_library_reopenable() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let fixture = Fixture::new();
    let (connection, game, complete, unknown) = schema_two(&fixture, false);
    connection
        .execute_batch(include_str!("../src/schema003.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    connection.execute_batch("CREATE TABLE interruption_padding(data BLOB); INSERT INTO interruption_padding VALUES(zeroblob(67108864));").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "migration_interruption_child"])
        .env(
            "RUSTRIS_MIGRATION_CHILD_ROOT",
            fixture.paths.data.parent().unwrap(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let pending = fs::read_dir(&fixture.paths.data).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".schema-3-backup-")
        });
        if pending {
            break;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "migration finished before interruption point"
        );
        assert!(
            Instant::now() < deadline,
            "migration did not start its backup"
        );
        std::thread::sleep(Duration::from_micros(200));
    }
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        connection
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    let store = Store::open(fixture.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    assert_eq!(
        store.session(complete).unwrap().state,
        SessionState::Cancelled
    );
    assert_eq!(store.session(unknown).unwrap().state, SessionState::Unknown);
    assert!(fs::read_dir(&fixture.paths.data).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-3-backup-")
    }));
    assert!(fs::read_dir(&fixture.paths.data).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".schema-3-backup-")
    }));
}
fn schema_two(fixture: &Fixture, conflict: bool) -> (rusqlite::Connection, Uuid, Uuid, Uuid) {
    fixture.paths.prepare().unwrap();
    let connection = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    connection
        .execute_batch(include_str!("../src/schema.sql"))
        .unwrap();
    connection
        .execute_batch(include_str!("../src/schema002.sql"))
        .unwrap();
    connection
        .pragma_update(None, "application_id", 0x5255_5354_i64)
        .unwrap();
    connection.pragma_update(None, "user_version", 2).unwrap();
    connection
        .pragma_update(None, "journal_mode", "WAL")
        .unwrap();
    fs::set_permissions(fixture.paths.database(), fs::Permissions::from_mode(0o600)).unwrap();
    let game = Uuid::now_v7();
    let installation = Uuid::now_v7();
    let profile = Uuid::now_v7();
    connection
        .execute(
            "INSERT INTO games(id,display_name,favorite) VALUES(?1,'Legacy 🦀',1)",
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
    connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?1,?2,?3,?4,'native')",rusqlite::params![profile.as_bytes().as_slice(),installation.as_bytes().as_slice(),b"/absent_\xff".as_slice(),b"/".as_slice()]).unwrap();
    let complete = Uuid::now_v7();
    let unknown = Uuid::now_v7();
    for (session, state, finished) in [
        (complete, "cancelled", Some(1001_i64)),
        (unknown, "unknown", None),
    ] {
        connection.execute("INSERT INTO launch_sessions(id,game_id,installation_id,profile_id,revision,display_name,runner,executable,cwd,environment_policy,state,created_ms,completed_ms,secret) VALUES(?1,?2,?3,?4,1,'Historical native','native',?5,?6,'desktop-allowlist-v1',?7,1000,?8,?9)",rusqlite::params![session.as_bytes().as_slice(),game.as_bytes().as_slice(),installation.as_bytes().as_slice(),profile.as_bytes().as_slice(),b"/absent_\xff".as_slice(),b"/".as_slice(),state,finished,Uuid::new_v4().as_bytes().as_slice()]).unwrap();
        connection
            .execute(
                "INSERT INTO session_arguments VALUES(?1,0,?2)",
                rusqlite::params![session.as_bytes().as_slice(), b"raw_\xfe".as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE launch_sessions SET sealed=1 WHERE id=?1",
                [session.as_bytes().as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO session_events VALUES(?1,0,1000,?2,'Legacy fixture state')",
                rusqlite::params![session.as_bytes().as_slice(), state],
            )
            .unwrap();
    }
    connection
        .execute(
            "INSERT INTO session_reservations(session_id,profile_id) VALUES(?1,?2)",
            rusqlite::params![unknown.as_bytes().as_slice(), profile.as_bytes().as_slice()],
        )
        .unwrap();
    if conflict {
        connection
            .execute("CREATE TABLE session_runners(fault INTEGER)", [])
            .unwrap();
    }
    (connection, game, complete, unknown)
}
#[test]
fn populated_schema_two_wal_preserves_history_and_unknown_reservation() {
    let fixture = Fixture::new();
    let (connection, game, complete, unknown) = schema_two(&fixture, false);
    let store = Store::open(fixture.paths.clone()).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    let historical = store.session(complete).unwrap();
    assert_eq!(historical.state, SessionState::Cancelled);
    assert!(historical.snapshot.runtime.is_none());
    assert_eq!(
        historical.snapshot.source.profile.arguments[0].as_bytes(),
        b"raw_\xfe"
    );
    assert_eq!(store.session(unknown).unwrap().state, SessionState::Unknown);
    assert!(
        store
            .remove(game)
            .unwrap_err()
            .to_string()
            .contains("active or unknown")
    );
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    let files: Vec<_> = fs::read_dir(&fixture.paths.data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-2-backup-")
        })
        .collect();
    assert_eq!(files.len(), 1);
    let backup = rusqlite::Connection::open(&files[0]).unwrap();
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM launch_sessions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        fs::metadata(&files[0]).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(store.snapshot().unwrap().entries[0].favorite);
}
#[test]
fn schema_three_ddl_failure_preserves_populated_v2_and_usable_backup() {
    let fixture = Fixture::new();
    let (connection, _, _, _) = schema_two(&fixture, true);
    let before = fs::read(fixture.paths.database()).unwrap();
    assert!(Store::open(fixture.paths.clone()).is_err());
    assert_eq!(fs::read(fixture.paths.database()).unwrap(), before);
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let backup = fs::read_dir(&fixture.paths.data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-2-backup-")
        })
        .unwrap();
    let backup = rusqlite::Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM launch_sessions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='session_preflights'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}
#[test]
fn populated_schema_three_upgrade_preserves_sessions_and_backup() {
    let fixture = Fixture::new();
    let (connection, game, complete, unknown) = schema_two(&fixture, false);
    connection
        .execute_batch(include_str!("../src/schema003.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    let store = Store::open(fixture.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    assert!(store.snapshot().unwrap().entries[0].artwork.is_none());
    assert_eq!(
        store.session(complete).unwrap().state,
        SessionState::Cancelled
    );
    assert_eq!(store.session(unknown).unwrap().state, SessionState::Unknown);
    let backup = fs::read_dir(&fixture.paths.data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-3-backup-")
        })
        .unwrap();
    let backup = rusqlite::Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        backup
            .query_row("SELECT count(*) FROM launch_sessions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
}
#[test]
fn verified_schema_three_backup_can_be_opened_in_a_separate_root() {
    let source = Fixture::new();
    let (connection, game, complete, unknown) = schema_two(&source, false);
    connection
        .execute_batch(include_str!("../src/schema003.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    let migrated = Store::open(source.paths.clone()).unwrap();
    let backup = fs::read_dir(&source.paths.data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-3-backup-")
        })
        .unwrap();
    let restored = Fixture::new();
    restored.paths.prepare().unwrap();
    fs::copy(&backup, restored.paths.database()).unwrap();
    let store = Store::open(restored.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    assert_eq!(
        store.session(complete).unwrap().state,
        SessionState::Cancelled
    );
    assert_eq!(store.session(unknown).unwrap().state, SessionState::Unknown);
    let restored_db = rusqlite::Connection::open(restored.paths.database()).unwrap();
    assert_eq!(
        restored_db
            .query_row(
                "SELECT count(*) FROM session_reservations WHERE session_id=?1",
                [unknown.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        migrated.session(unknown).unwrap().state,
        SessionState::Unknown
    );
}
#[test]
fn killed_uncommitted_schema_change_rolls_back_and_reopens() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let source = Fixture::new();
    let (connection, game, complete, unknown) = schema_two(&source, false);
    connection
        .execute_batch(include_str!("../src/schema003.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    let _migrated = Store::open(source.paths.clone()).unwrap();
    let backup = fs::read_dir(&source.paths.data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-3-backup-")
        })
        .unwrap();
    let restored = Fixture::new();
    restored.paths.prepare().unwrap();
    fs::copy(backup, restored.paths.database()).unwrap();
    let child_root = restored.paths.data.parent().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "uncommitted_ddl_child"])
        .env("RUSTRIS_DDL_CHILD_ROOT", child_root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !child_root.join("ddl-ready").exists()
        && child.try_wait().unwrap().is_none()
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let ready = child_root.join("ddl-ready").exists();
    let mut wal_path = restored.paths.database().into_os_string();
    wal_path.push("-wal");
    let spilled = fs::metadata(wal_path).is_ok_and(|metadata| metadata.len() > 32);
    let (killed, status) = match child.try_wait().unwrap() {
        Some(status) => (false, status),
        None => {
            child.kill().unwrap();
            (true, child.wait().unwrap())
        }
    };
    assert!(ready, "DDL child did not reach its checkpoint");
    assert!(spilled, "No uncommitted WAL pages were observed");
    assert!(killed && !status.success(), "DDL child was not interrupted");
    let database = rusqlite::Connection::open(restored.paths.database()).unwrap();
    assert_eq!(
        database
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        database
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(database.query_row("SELECT count(*) FROM sqlite_master WHERE name IN ('local_artwork','interruption_padding')", [], |r| r.get::<_, i64>(0)).unwrap(), 0);
    drop(database);
    let store = Store::open(restored.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    assert_eq!(
        store.session(complete).unwrap().state,
        SessionState::Cancelled
    );
    assert_eq!(store.session(unknown).unwrap().state, SessionState::Unknown);
    let reopened = rusqlite::Connection::open(restored.paths.database()).unwrap();
    assert_eq!(
        reopened
            .query_row(
                "SELECT count(*) FROM session_reservations WHERE session_id=?1",
                [unknown.as_bytes().as_slice()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        reopened
            .query_row(
                "SELECT value FROM session_arguments WHERE session_id=?1",
                [unknown.as_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
        b"raw_\xfe"
    );
    assert!(fs::read_dir(&restored.paths.data).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-3-backup-")
    }));
}
#[test]
fn unavailable_backup_directory_blocks_migration_and_preserves_library() {
    assert_ne!(
        rustix::process::geteuid().as_raw(),
        0,
        "This permission fixture requires an unprivileged user"
    );
    let fixture = Fixture::new();
    let (connection, game, complete, unknown) = schema_two(&fixture, false);
    connection
        .execute_batch(include_str!("../src/schema003.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    fs::set_permissions(&fixture.paths.data, fs::Permissions::from_mode(0o500)).unwrap();
    let outcome = Store::open(fixture.paths.clone());
    fs::set_permissions(&fixture.paths.data, fs::Permissions::from_mode(0o700)).unwrap();
    let error = match outcome {
        Err(error) => error,
        Ok(_) => panic!("The migration proceeded without permission to create its backup"),
    };
    assert!(
        error.to_string().contains("create the migration backup"),
        "{error}"
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        connection
            .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='local_artwork'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert!(!fs::read_dir(&fixture.paths.data).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("schema-3-backup-")
    }));
    let store = Store::open(fixture.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    assert_eq!(
        store.session(complete).unwrap().state,
        SessionState::Cancelled
    );
    assert_eq!(store.session(unknown).unwrap().state, SessionState::Unknown);
    assert!(fs::read_dir(&fixture.paths.data).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-3-backup-")
    }));
}
#[test]
fn schema_four_failure_keeps_original_and_verified_schema_three_backup() {
    let fixture = Fixture::new();
    let (connection, _, _, _) = schema_two(&fixture, false);
    connection
        .execute_batch(include_str!("../src/schema003.sql"))
        .unwrap();
    connection.pragma_update(None, "user_version", 3).unwrap();
    connection
        .execute("CREATE TABLE local_artwork(conflict INTEGER)", [])
        .unwrap();
    assert!(Store::open(fixture.paths.clone()).is_err());
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM launch_sessions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    let backup = fs::read_dir(&fixture.paths.data)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-3-backup-")
        })
        .unwrap();
    let backup = rusqlite::Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
}
fn report(count: usize, message: usize) -> Preflight {
    let observations: Vec<_> = (0..count)
        .map(|i| Observation {
            id: Uuid::now_v7(),
            resource: format!("resource_{i}"),
            source: "metadata and bounded manifest read".into(),
            observed_ms: 1700000000000,
            outcome: "unsupported".into(),
            errno: None,
            device: None,
            inode: None,
            size: None,
            modified_seconds: None,
            modified_nanoseconds: None,
        })
        .collect();
    let findings = observations
        .iter()
        .map(|o| Finding {
            rule: format!("umu.{}.preparation", o.resource),
            evidence: o.id,
            message: "x".repeat(message),
        })
        .collect();
    Preflight {
        observations,
        findings,
    }
}
#[test]
fn large_preflight_is_linked_append_only_and_bounded_separately_from_events() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let (session, owner) = store.reserve_session(game, 1, &Default::default()).unwrap();
    let report = report(24, 300);
    let length = serde_json::to_string(&report).unwrap().len();
    assert!(length > 4096 && length < 32768);
    let recorded = store
        .record_preflight(&owner, session.generation, &report)
        .unwrap();
    assert_eq!(recorded.generation, 1);
    assert!(recorded.detail.len() < 4096);
    assert_eq!(
        store
            .native_preflight(session.id)
            .unwrap()
            .unwrap()
            .findings
            .len(),
        24
    );
    assert!(matches!(
        store.record_preflight(&owner, 0, &report),
        Err(Error::SessionChanged)
    ));
    let connection = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    assert!(
        connection
            .execute("UPDATE session_preflights SET report='{}'", [])
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM session_preflights", [])
            .is_err()
    );
    let oversized = report_with_bad_link();
    assert!(store.record_preflight(&owner, 1, &oversized).is_err());
    assert!(
        store
            .record_preflight(&owner, 1, &super_large_report())
            .is_err()
    );
    assert_eq!(store.session(session.id).unwrap().generation, 1);
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM session_preflights", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
fn report_with_bad_link() -> Preflight {
    let mut value = report(1, 1);
    value.findings[0].evidence = Uuid::new_v4();
    value
}
fn super_large_report() -> Preflight {
    report(32, 2048)
}
