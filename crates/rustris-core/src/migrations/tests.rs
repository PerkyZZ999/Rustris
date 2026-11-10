#![expect(
    clippy::unwrap_used,
    reason = "Private migration interruption fixtures"
)]

use super::*;
use crate::{paths::Paths, sessions::SessionState, store::Store};
use rusqlite::params;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

const CHECKPOINT: &str = "RUSTRIS_TEST_MIGRATION_CHECKPOINT";
const ROOT: &str = "RUSTRIS_TEST_MIGRATION_ROOT";
const BACKUP_FAULT: &str = "RUSTRIS_TEST_BACKUP_FAULT";

pub(super) fn backup_fault(stage: &str) -> Result<()> {
    if std::env::var(BACKUP_FAULT).ok().as_deref() == Some(stage) {
        let action = match stage {
            "sync" => "sync the migration backup",
            "publish" => "publish the verified migration backup",
            "directory-sync" => "sync the migration backup directory",
            _ => "complete the migration backup",
        };
        return Err(io(action, std::io::Error::other("injected fault")));
    }
    Ok(())
}

pub(super) fn checkpoint(paths: &Paths, stage: &str) {
    if std::env::var(CHECKPOINT).ok().as_deref() != Some(stage) {
        return;
    }
    if let Err(error) = fs::write(paths.data.join("migration-test-ready"), stage) {
        panic!("Cannot publish the private migration checkpoint: {error}");
    }
    loop {
        std::thread::park();
    }
}

struct Fixture {
    root: PathBuf,
    paths: Paths,
    game: Uuid,
    session: Uuid,
    source: Option<Connection>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("rustris-migration-{}", Uuid::new_v4()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let paths = Paths::in_root(&root);
        paths.prepare().unwrap();
        let connection = Connection::open(paths.database()).unwrap();
        connection
            .execute_batch(include_str!("../schema.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../schema002.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../schema003.sql"))
            .unwrap();
        connection
            .pragma_update(None, "application_id", APP_ID)
            .unwrap();
        connection.pragma_update(None, "user_version", 3).unwrap();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        fs::set_permissions(paths.database(), fs::Permissions::from_mode(0o600)).unwrap();
        let game = Uuid::new_v4();
        let installation = Uuid::new_v4();
        let profile = Uuid::new_v4();
        let session = Uuid::new_v4();
        connection
            .execute(
                "INSERT INTO games(id,display_name,favorite) VALUES(?1,'Migration fixture',1)",
                [game.as_bytes().as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO installations(id,game_id) VALUES(?1,?2)",
                params![
                    installation.as_bytes().as_slice(),
                    game.as_bytes().as_slice()
                ],
            )
            .unwrap();
        connection.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner) VALUES(?1,?2,?3,?4,'native')", params![profile.as_bytes().as_slice(), installation.as_bytes().as_slice(), b"/missing_\xff".as_slice(), b"/".as_slice()]).unwrap();
        connection.execute("INSERT INTO launch_sessions(id,game_id,installation_id,profile_id,revision,display_name,runner,executable,cwd,environment_policy,state,created_ms,secret) VALUES(?1,?2,?3,?4,1,'Historical','native',?5,?6,'desktop-allowlist-v1','unknown',1000,?7)", params![session.as_bytes().as_slice(), game.as_bytes().as_slice(), installation.as_bytes().as_slice(), profile.as_bytes().as_slice(), b"/missing_\xff".as_slice(), b"/".as_slice(), Uuid::new_v4().as_bytes().as_slice()]).unwrap();
        connection
            .execute(
                "INSERT INTO session_arguments VALUES(?1,0,?2)",
                params![session.as_bytes().as_slice(), b"raw_\xfe".as_slice()],
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
                "INSERT INTO session_events VALUES(?1,0,1000,'unknown','Fixture')",
                [session.as_bytes().as_slice()],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO session_reservations(session_id,profile_id) VALUES(?1,?2)",
                params![session.as_bytes().as_slice(), profile.as_bytes().as_slice()],
            )
            .unwrap();
        Self {
            root,
            paths,
            game,
            session,
            source: Some(connection),
        }
    }
}

#[test]
fn backup_sync_and_publish_faults_block_migration_until_retry() {
    for stage in ["sync", "publish", "directory-sync"] {
        let fixture = Fixture::new();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "migrations::tests::backup_fault_child",
            ])
            .env(ROOT, &fixture.root)
            .env(BACKUP_FAULT, stage)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let result_path = fixture.root.join("fault-result");
        let deadline = Instant::now() + Duration::from_secs(15);
        while !result_path.exists()
            && child.try_wait().unwrap().is_none()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let status = child.wait().unwrap();
        let message = fs::read_to_string(&result_path).unwrap_or_default();
        let expected = match stage {
            "sync" => "Cannot sync the migration backup",
            "publish" => "Cannot publish the verified migration backup",
            "directory-sync" => "Cannot sync the migration backup directory",
            _ => unreachable!(),
        };
        assert!(
            status.success() && message.contains(expected),
            "{stage} produced {status}: {message}"
        );
        assert_library(&fixture.paths.database(), &fixture, 3);
        let published = backups(&fixture.paths);
        if stage == "directory-sync" {
            assert_eq!(published.len(), 1, "{stage}");
            assert_library(&published[0], &fixture, 3);
        } else {
            assert!(published.is_empty(), "{stage}: {published:?}");
        }
        let store = Store::open(fixture.paths.clone()).unwrap();
        assert_library(&fixture.paths.database(), &fixture, SCHEMA);
        assert_eq!(store.snapshot().unwrap().entries[0].game_id, fixture.game);
        assert_eq!(
            store.session(fixture.session).unwrap().state,
            SessionState::Unknown
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        drop(self.source.take());
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
#[ignore = "Invoked only by the private migration checkpoint test"]
fn checkpoint_child() {
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    Store::open(Paths::in_root(&root)).unwrap();
    panic!("The requested migration checkpoint was not reached");
}

#[test]
#[ignore = "Invoked only by the backup fault fixture"]
fn backup_fault_child() {
    let root = PathBuf::from(std::env::var_os(ROOT).unwrap());
    let message = match Store::open(Paths::in_root(&root)) {
        Ok(_) => "opened".to_string(),
        Err(error) => error.to_string(),
    };
    fs::write(root.join("fault-result"), message).unwrap();
}

fn backups(paths: &Paths) -> Vec<PathBuf> {
    fs::read_dir(&paths.data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-3-backup-")
                && path
                    .extension()
                    .is_some_and(|extension| extension == "sqlite3")
        })
        .collect()
}

fn assert_library(path: &Path, fixture: &Fixture, version: i64) {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
            .unwrap(),
        version
    );
    assert_eq!(
        connection
            .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='local_artwork'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        i64::from(version >= 4)
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT display_name FROM games WHERE id=?1",
                [fixture.game.as_bytes().as_slice()],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "Migration fixture"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT value FROM session_arguments WHERE session_id=?1",
                [fixture.session.as_bytes().as_slice()],
                |row| row.get::<_, Vec<u8>>(0)
            )
            .unwrap(),
        b"raw_\xfe"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM session_reservations WHERE session_id=?1",
                [fixture.session.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}

#[test]
fn interruption_at_production_migration_checkpoints_preserves_library() {
    for stage in ["backup-published", "ddl-staged", "commit-completed"] {
        let fixture = Fixture::new();
        let mut wal = fixture.paths.database().into_os_string();
        wal.push("-wal");
        assert!(fs::metadata(wal).unwrap().len() > 32);
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "migrations::tests::checkpoint_child",
            ])
            .env(ROOT, &fixture.root)
            .env(CHECKPOINT, stage)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let ready_path = fixture.paths.data.join("migration-test-ready");
        let deadline = Instant::now() + Duration::from_secs(15);
        while !ready_path.exists()
            && child.try_wait().unwrap().is_none()
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let ready = fs::read_to_string(&ready_path).ok().as_deref() == Some(stage);
        let (killed, status) = match child.try_wait().unwrap() {
            Some(status) => (false, status),
            None => {
                child.kill().unwrap();
                (true, child.wait().unwrap())
            }
        };
        assert!(
            ready && killed && !status.success(),
            "Migration did not stop at {stage}"
        );
        let before_retry = backups(&fixture.paths);
        assert_eq!(before_retry.len(), 1);
        let version = if stage == "commit-completed" {
            SCHEMA
        } else {
            3
        };
        assert_library(&fixture.paths.database(), &fixture, version);
        assert_library(&before_retry[0], &fixture, 3);
        let store = Store::open(fixture.paths.clone()).unwrap();
        assert_eq!(store.snapshot().unwrap().entries[0].game_id, fixture.game);
        assert_eq!(
            store.session(fixture.session).unwrap().state,
            SessionState::Unknown
        );
        assert_library(&fixture.paths.database(), &fixture, SCHEMA);
        let after_retry = backups(&fixture.paths);
        assert_eq!(after_retry.len(), if version == SCHEMA { 1 } else { 2 });
        for backup in &after_retry {
            assert_library(backup, &fixture, 3);
        }
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT count(*) FROM session_reservations WHERE session_id=?1",
                    [fixture.session.as_bytes().as_slice()],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
    }
}
