#![expect(clippy::unwrap_used, reason = "Explicit fixture assertions")]
use rustris_core::{
    Error,
    bytes::OsBytes,
    domain::{EnvDelta, Runner},
    paths::Settings,
    store::Store,
    worker::{Command, Response, Worker},
};
use std::{fs, os::unix::fs::PermissionsExt};

mod common;
use common::Fixture;

#[test]
fn exact_profile_commit_edit_favorite_selection_and_reopen() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let mut input = f.input();
    let game = store.save(None, &input).unwrap();
    store.favorite(game, true).unwrap();
    store.select(game).unwrap();
    let row = store.snapshot().unwrap().entries.remove(0);
    assert_eq!(row.profile, input);
    assert_ne!(row.game_id, row.installation_id);
    assert_ne!(row.installation_id, row.profile_id);
    input.name = "Changed".into();
    store.save(Some((game, 1)), &input).unwrap();
    assert!(matches!(
        store.save(Some((game, 1)), &input),
        Err(Error::Conflict)
    ));
    drop(store);
    let store = Store::open(f.paths.clone()).unwrap();
    let snapshot = store.snapshot().unwrap();
    let entry = &snapshot.entries[0];
    assert_eq!(entry.profile, input);
    assert_eq!(entry.revision, 2);
    assert!(entry.favorite);
    assert_eq!(snapshot.selected, Some(game));
    let sql = rusqlite::Connection::open(f.paths.database()).unwrap();
    let types: (String, String) = sql
        .query_row(
            "SELECT typeof(executable),typeof(cwd) FROM profiles",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(types, ("blob".into(), "blob".into()));
    store.remove(game).unwrap();
    assert!(store.snapshot().unwrap().entries.is_empty());
    assert!(f.root.join("foreign-game").exists());
    assert!(!f.root.join("never").exists());
}

#[test]
fn local_artwork_round_trips_bytes_and_never_changes_external_file_or_profile_revision() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let game = store.save(None, &f.input()).unwrap();
    let path = OsBytes::new(b"/missing/art_\xff.png").unwrap();
    store.set_artwork(game, Some(&path)).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].revision, 1);
    drop(store);
    let mut store = Store::open(f.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].artwork, Some(path));
    assert!(
        store
            .set_artwork(game, Some(&OsBytes::new(b"relative.png").unwrap()))
            .is_err()
    );
    let file = f.root.join("image.png");
    fs::write(&file, b"user owned fixture").unwrap();
    use std::os::unix::ffi::OsStrExt;
    let file_path = OsBytes::new(file.as_os_str().as_bytes()).unwrap();
    store.set_artwork(game, Some(&file_path)).unwrap();
    store.set_artwork(game, None).unwrap();
    assert!(store.snapshot().unwrap().entries[0].artwork.is_none());
    assert_eq!(fs::read(file).unwrap(), b"user owned fixture");
    assert!(matches!(
        store.set_artwork(uuid::Uuid::new_v4(), Some(&file_path)),
        Err(Error::NotFound)
    ));
}
#[test]
fn unavailable_or_oversized_artwork_is_not_presented_for_decoding() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let game = store.save(None, &f.input()).unwrap();
    use std::os::unix::ffi::OsStrExt;
    let missing = OsBytes::new(f.root.join("missing.png").as_os_str().as_bytes()).unwrap();
    store.set_artwork(game, Some(&missing)).unwrap();
    assert!(!store.snapshot().unwrap().entries[0].artwork_available);
    let oversized_path = f.root.join("oversized.png");
    fs::File::create(&oversized_path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    let oversized = OsBytes::new(oversized_path.as_os_str().as_bytes()).unwrap();
    store.set_artwork(game, Some(&oversized)).unwrap();
    assert!(!store.snapshot().unwrap().entries[0].artwork_available);
    assert_eq!(
        store.snapshot().unwrap().entries[0].artwork,
        Some(oversized)
    );
}
#[test]
fn transaction_failure_leaves_no_partial_entry_or_profile() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let sql = rusqlite::Connection::open(f.paths.database()).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_fixture BEFORE INSERT ON arguments BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;").unwrap();
    assert!(store.save(None, &f.input()).is_err());
    for table in [
        "games",
        "installations",
        "profiles",
        "arguments",
        "environment",
    ] {
        let count: i64 = sql
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[test]
fn invalid_environment_and_prefix_payload_never_create_records() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    for key in [b"".as_slice(), b"BAD=KEY"] {
        let mut input = f.input();
        input.environment = vec![EnvDelta {
            key: OsBytes::new(key).unwrap(),
            value: None,
        }];
        assert!(matches!(store.save(None, &input), Err(Error::Invalid(_))));
    }
    let mut input = f.input();
    input.environment.push(input.environment[0].clone());
    assert!(matches!(store.save(None, &input), Err(Error::Invalid(_))));

    // Exactly 1 MiB before the required prefix: its byte must count too.
    let mut absolute = vec![b'x'; 64 * 1024];
    absolute[0] = b'/';
    input.executable = OsBytes::new(absolute.clone()).unwrap();
    input.cwd = OsBytes::new(absolute).unwrap();
    input.arguments = vec![OsBytes::new(vec![b'x'; 64 * 1024]).unwrap(); 13];
    input.environment = vec![EnvDelta {
        key: OsBytes::new(b"K").unwrap(),
        value: Some(OsBytes::new(vec![b'x'; 64 * 1024 - 1]).unwrap()),
    }];
    input.runner = Runner::Umu;
    input.prefix = Some(OsBytes::new(b"/").unwrap());
    assert!(matches!(store.save(None, &input), Err(Error::Invalid(_))));
    assert!(store.snapshot().unwrap().entries.is_empty());
}

#[test]
fn reopen_does_not_replace_a_newer_library_with_an_older_backup() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let mut input = f.input();
    input.name = "Newer live library".into();
    store.save(None, &input).unwrap();
    drop(store);
    let backup = f.paths.data.join("schema-1-backup-older.sqlite3");
    fs::write(&backup, b"older-backup-not-a-library").unwrap();
    let store = Store::open(f.paths.clone()).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.entries[0].profile.name, "Newer live library");
    assert_eq!(fs::read(&backup).unwrap(), b"older-backup-not-a-library");
    let version: i64 = rusqlite::Connection::open(f.paths.database())
        .unwrap()
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 5);
}

#[test]
fn unknown_corrupt_and_newer_databases_are_not_rewritten() {
    for kind in ["foreign", "corrupt", "newer"] {
        let f = Fixture::new();
        f.paths.prepare().unwrap();
        match kind {
            "foreign" => {
                let sql = rusqlite::Connection::open(f.paths.database()).unwrap();
                sql.execute_batch(
                    "CREATE TABLE keep(value);INSERT INTO keep VALUES('synthetic-owned-marker');",
                )
                .unwrap();
            }
            "corrupt" => fs::write(f.paths.database(), b"Not a database\xff\x00").unwrap(),
            _ => {
                let mut store = Store::open(f.paths.clone()).unwrap();
                store.save(None, &f.input()).unwrap();
                drop(store);
                let sql = rusqlite::Connection::open(f.paths.database()).unwrap();
                sql.pragma_update(None, "user_version", 99).unwrap();
            }
        }
        fs::set_permissions(f.paths.database(), fs::Permissions::from_mode(0o600)).unwrap();
        let before = fs::read(f.paths.database()).unwrap();
        assert!(Store::open(f.paths.clone()).is_err(), "{kind}");
        assert_eq!(fs::read(f.paths.database()).unwrap(), before);
    }
}

#[test]
fn readonly_library_reports_failed_write_without_changing_committed_data() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    store.save(None, &f.input()).unwrap();
    drop(store);
    let before = fs::read(f.paths.database()).unwrap();
    fs::set_permissions(f.paths.database(), fs::Permissions::from_mode(0o400)).unwrap();
    if let Ok(mut store) = Store::open(f.paths.clone()) {
        assert!(store.save(None, &f.input()).is_err());
    }
    assert_eq!(fs::read(f.paths.database()).unwrap(), before);
}

#[test]
fn bad_preferences_are_visible_and_preserved_without_blocking_library_reads() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    store.save(None, &f.input()).unwrap();
    store
        .preferences(Settings {
            opaque: true,
            reduced_motion: true,
        })
        .unwrap();
    assert!(store.snapshot().unwrap().settings.opaque);
    let bad = b"unknown_future_setting = [broken";
    fs::write(f.paths.preferences(), bad).unwrap();
    let snapshot = store.snapshot().unwrap();
    assert!(snapshot.warning.is_some());
    assert_eq!(snapshot.entries.len(), 1);
    assert!(store.preferences(Settings::default()).is_err());
    assert_eq!(fs::read(f.paths.preferences()).unwrap(), bad);
}

#[test]
fn view_drop_does_not_cancel_an_accepted_transaction() {
    let f = Fixture::new();
    let worker = Worker::start(f.paths.clone()).unwrap();
    let client = worker.client();
    let response = client.submit(Command::Save(None, f.input())).unwrap();
    drop(response);
    worker.shutdown().unwrap();
    assert_eq!(
        Store::open(f.paths.clone())
            .unwrap()
            .snapshot()
            .unwrap()
            .entries
            .len(),
        1
    );
}

#[test]
fn post_commit_reload_failure_is_distinguished_from_rollback() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let game = store.save(None, &f.input()).unwrap();
    drop(store);
    let sql = rusqlite::Connection::open(f.paths.database()).unwrap();
    sql.execute_batch("CREATE TRIGGER corrupt_fixture AFTER UPDATE OF executable ON profiles BEGIN UPDATE profiles SET cwd=X'00' WHERE id=NEW.id; END;").unwrap();
    let worker = Worker::start(f.paths.clone()).unwrap();
    let client = worker.client();
    // First load establishes the known, valid connection before the injected update.
    assert!(matches!(
        client
            .submit(Command::Load)
            .unwrap()
            .blocking_recv()
            .unwrap()
            .unwrap(),
        Response::Snapshot(_)
    ));
    let error = client
        .submit(Command::Save(Some((game, 1)), f.input()))
        .unwrap()
        .blocking_recv()
        .unwrap()
        .err()
        .unwrap();
    assert!(matches!(error, Error::CommittedRefresh(_)));
    worker.shutdown().unwrap();
    let revision: i64 = sql
        .query_row("SELECT revision FROM profiles", [], |r| r.get(0))
        .unwrap();
    assert_eq!(revision, 2);
}

#[test]
fn approved_review_round_trips_replaces_and_cascades_with_the_game() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let game = store.save(None, &f.input()).unwrap();
    assert_eq!(store.approved_review(game).unwrap(), None);
    assert!(!store.snapshot().unwrap().entries[0].review_approved);
    store.approve_review(game, &[7; 32]).unwrap();
    store.approve_review(game, &[9; 32]).unwrap();
    assert_eq!(store.approved_review(game).unwrap(), Some(vec![9; 32]));
    assert!(store.snapshot().unwrap().entries[0].review_approved);
    // Approval is not a profile edit: the revision and conflict checks are unchanged.
    assert_eq!(store.snapshot().unwrap().entries[0].revision, 1);
    drop(store);
    let mut store = Store::open(f.paths.clone()).unwrap();
    assert_eq!(store.approved_review(game).unwrap(), Some(vec![9; 32]));
    // Editing the profile, or choosing new runners, asks for review again.
    store.save(Some((game, 1)), &f.input()).unwrap();
    assert_eq!(store.approved_review(game).unwrap(), None);
    store.approve_review(game, &[3; 32]).unwrap();
    store.clear_approved_reviews().unwrap();
    assert_eq!(store.approved_review(game).unwrap(), None);
    store.approve_review(game, &[3; 32]).unwrap();
    store.remove(game).unwrap();
    let rows: i64 = rusqlite::Connection::open(f.paths.database())
        .unwrap()
        .query_row("SELECT count(*) FROM approved_reviews", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    assert!(matches!(
        store.approve_review(game, &[1; 32]),
        Err(Error::NotFound)
    ));
}

#[test]
fn schema_four_library_upgrades_to_five_after_a_verified_backup() {
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let game = store.save(None, &f.input()).unwrap();
    drop(store);
    // Recreate the schema-4 shape: drop the schema-5 tables and lower the version.
    let sql = rusqlite::Connection::open(f.paths.database()).unwrap();
    sql.execute_batch(
        "DROP TABLE approved_reviews; DROP TABLE fetched_artwork; PRAGMA user_version=4;",
    )
    .unwrap();
    drop(sql);
    let store = Store::open(f.paths.clone()).unwrap();
    assert_eq!(store.snapshot().unwrap().entries[0].game_id, game);
    let backups: Vec<_> = fs::read_dir(&f.paths.data)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.starts_with("schema-4-backup-") && name.ends_with(".sqlite3"))
        .collect();
    assert_eq!(backups.len(), 1, "{backups:?}");
    let backup = rusqlite::Connection::open(f.paths.data.join(&backups[0])).unwrap();
    let old: i64 = backup
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(old, 4);
    let live = rusqlite::Connection::open(f.paths.database()).unwrap();
    let version: i64 = live
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, 5);
    let tables: i64 = live
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name IN ('approved_reviews','fetched_artwork')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tables, 2);
}

#[test]
fn downloaded_artwork_replaces_its_own_file_and_is_removed_with_the_game() {
    use rustris_core::artwork::{self, Fetched, Kind};
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new();
    let worker = Worker::start(f.paths.clone()).unwrap();
    let client = worker.client();
    let game = match client
        .submit(Command::Save(None, f.input()))
        .unwrap()
        .blocking_recv()
        .unwrap()
        .unwrap()
    {
        Response::Snapshot(snapshot) => snapshot.entries[0].game_id,
        Response::SelectionSaved => unreachable!(),
    };
    let owned = artwork::directory(&f.paths, game);
    fs::create_dir_all(&owned).unwrap();
    let first = owned.join("cover-1.png");
    let second = owned.join("cover-2.png");
    let hero = owned.join("hero-1.jpg");
    for path in [&first, &second, &hero] {
        fs::write(path, b"\x89PNG\r\n\x1a\nfixture").unwrap();
    }
    let record = |files: Vec<Fetched>| match client
        .submit(Command::Fetched(game, files))
        .unwrap()
        .blocking_recv()
        .unwrap()
        .unwrap()
    {
        Response::Snapshot(snapshot) => snapshot.entries.into_iter().next().unwrap(),
        Response::SelectionSaved => unreachable!(),
    };
    let entry = record(vec![
        Fetched {
            kind: Kind::Cover,
            path: first.clone(),
            source: "steamgriddb",
        },
        Fetched {
            kind: Kind::Hero,
            path: hero.clone(),
            source: "steam",
        },
    ]);
    assert_eq!(
        entry.fetched_cover.unwrap().as_bytes(),
        first.as_os_str().as_bytes()
    );
    assert_eq!(
        entry.fetched_hero.unwrap().as_bytes(),
        hero.as_os_str().as_bytes()
    );
    assert!(
        entry.artwork.is_none(),
        "a download is never recorded as user artwork"
    );
    // A newer download replaces Rustris's own previous file.
    let entry = record(vec![Fetched {
        kind: Kind::Cover,
        path: second.clone(),
        source: "steam",
    }]);
    assert_eq!(
        entry.fetched_cover.unwrap().as_bytes(),
        second.as_os_str().as_bytes()
    );
    assert!(!first.exists() && second.exists());
    // A user-chosen image is kept separately and is never deleted.
    let mine = f.root.join("my-cover.png");
    fs::write(&mine, b"\x89PNG\r\n\x1a\nmine").unwrap();
    client
        .submit(Command::Artwork(
            game,
            Some(OsBytes::new(mine.as_os_str().as_bytes()).unwrap()),
        ))
        .unwrap()
        .blocking_recv()
        .unwrap()
        .unwrap();
    client
        .submit(Command::Remove(game))
        .unwrap()
        .blocking_recv()
        .unwrap()
        .unwrap();
    worker.shutdown().unwrap();
    assert!(!owned.exists(), "downloaded artwork goes with the game");
    assert!(mine.exists(), "the user's own image is untouched");
}

#[test]
fn artwork_settings_keep_the_key_private_and_report_only_status() {
    use rustris_core::artwork::{self, Change, KeyChange};
    use std::os::unix::fs::MetadataExt;
    let f = Fixture::new();
    Store::open(f.paths.clone()).unwrap();
    assert_eq!(
        artwork::status(&f.paths).unwrap(),
        artwork::Status::default()
    );
    assert!(
        artwork::change_settings(
            &f.paths,
            Change {
                enabled: true,
                key: Some(KeyChange::Set("bad key!".into()))
            }
        )
        .is_err()
    );
    artwork::change_settings(
        &f.paths,
        Change {
            enabled: true,
            key: Some(KeyChange::Set("abc123DEF".into())),
        },
    )
    .unwrap();
    let status = artwork::status(&f.paths).unwrap();
    assert!(status.enabled && status.has_key);
    let file = f.paths.config.join("artwork.toml");
    assert_eq!(fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
    // Changing only the switch keeps the key; clearing removes it.
    artwork::change_settings(
        &f.paths,
        Change {
            enabled: false,
            key: None,
        },
    )
    .unwrap();
    assert!(artwork::status(&f.paths).unwrap().has_key);
    artwork::change_settings(
        &f.paths,
        Change {
            enabled: false,
            key: Some(KeyChange::Clear),
        },
    )
    .unwrap();
    assert!(!artwork::status(&f.paths).unwrap().has_key);
    assert!(
        artwork::fetch(
            &f.paths,
            uuid::Uuid::now_v7(),
            "x",
            &[],
            &Default::default()
        )
        .is_err(),
        "nothing is fetched while the feature is off"
    );
}

#[test]
fn late_downloads_for_removed_games_and_leftovers_are_cleaned_up() {
    use rustris_core::artwork::{self, Fetched, Kind};
    use std::os::unix::ffi::OsStrExt;
    let f = Fixture::new();
    let mut store = Store::open(f.paths.clone()).unwrap();
    let kept = store.save(None, &f.input()).unwrap();
    let removed = store.save(None, &f.input()).unwrap();
    drop(store);
    let png = b"\x89PNG\r\n\x1a\nfixture";
    // A download that lands after its game was removed is deleted, quietly.
    let worker = Worker::start(f.paths.clone()).unwrap();
    let client = worker.client();
    client
        .submit(Command::Remove(removed))
        .unwrap()
        .blocking_recv()
        .unwrap()
        .unwrap();
    let late = artwork::directory(&f.paths, removed);
    fs::create_dir_all(&late).unwrap();
    fs::write(late.join("cover-late.png"), png).unwrap();
    let result = client
        .submit(Command::Fetched(
            removed,
            vec![Fetched {
                kind: Kind::Cover,
                path: late.join("cover-late.png"),
                source: "steam",
            }],
        ))
        .unwrap()
        .blocking_recv()
        .unwrap();
    assert!(
        result.is_ok(),
        "a removed game is not an error for the user"
    );
    assert!(!late.exists());
    worker.shutdown().unwrap();
    // Leftovers from a crash: an orphan folder, an unrecorded file and a partial
    // download are swept when the library opens; the user's own image is kept.
    let orphan = artwork::directory(&f.paths, uuid::Uuid::now_v7());
    let owned = artwork::directory(&f.paths, kept);
    fs::create_dir_all(&orphan).unwrap();
    fs::create_dir_all(&owned).unwrap();
    fs::write(orphan.join("cover-x.png"), png).unwrap();
    fs::write(owned.join("cover-unrecorded.png"), png).unwrap();
    fs::write(owned.join(".cover-y.png.partial"), b"partial").unwrap();
    let mine = owned.join("chosen-by-user.png");
    fs::write(&mine, png).unwrap();
    let mut store = Store::open(f.paths.clone()).unwrap();
    store
        .set_artwork(
            kept,
            Some(&OsBytes::new(mine.as_os_str().as_bytes()).unwrap()),
        )
        .unwrap();
    store.sweep_owned_artwork();
    assert!(!orphan.exists());
    assert!(!owned.join("cover-unrecorded.png").exists());
    assert!(!owned.join(".cover-y.png.partial").exists());
    assert!(mine.exists(), "a file the user chose is never swept");
}
