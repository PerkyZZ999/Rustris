use crate::{
    Error, Result,
    bytes::OsBytes,
    domain::{Entry, EnvDelta, ProfileInput, Runner},
    error::io,
    migrations::{self, APP_ID, SCHEMA},
    paths::{Paths, Settings},
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::Duration,
};
use uuid::Uuid;

/// Owned persisted state returned only after commits complete.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub entries: Vec<Entry>,
    /// D-011: whether automatic artwork is on and a SteamGridDB key is stored.
    pub artwork: crate::artwork::Status,
    pub selected: Option<Uuid>,
    pub settings: Settings,
    pub warning: Option<String>,
}

/// A connection exclusively owned by its calling storage worker/thread.
pub struct Store {
    pub(crate) connection: Connection,
    pub(crate) paths: Paths,
}

impl Store {
    /// Open private owned state; inspect existing nonempty databases read-only first.
    pub fn open(paths: Paths) -> Result<Self> {
        Self::open_mode(paths, true)
    }
    /// Background services require the original library and never recreate missing state.
    pub fn open_existing(paths: Paths) -> Result<Self> {
        Self::open_mode(paths, false)
    }
    fn open_mode(paths: Paths, create: bool) -> Result<Self> {
        if create {
            paths.prepare()?;
        } else {
            crate::paths::verify_private_directory(&paths.data)?;
            crate::paths::verify_private_directory(&paths.config)?;
        }
        let db = paths.database();
        let initial = match fs::symlink_metadata(&db) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                OpenOptions::new()
                    .create_new(true)
                    .write(true)
                    .mode(0o600)
                    .open(&db)
                    .map_err(|e| io("create the library database", e))?;
                true
            }
            Err(e) => return Err(io("inspect the library database", e)),
            Ok(meta) => {
                if !meta.is_file()
                    || meta.uid() != rustix::process::geteuid().as_raw()
                    || meta.mode() & 0o077 != 0
                {
                    return Err(Error::Database(
                        "The library must be a private, owned regular file (0600), not a symlink"
                            .into(),
                    ));
                }
                if meta.len() == 0 && !create {
                    return Err(Error::Database(
                        "The existing library is empty; background recovery did not initialize it"
                            .into(),
                    ));
                }
                meta.len() == 0
            }
        };
        if !initial {
            inspect_existing(&db, &paths)?;
        }
        let mut connection = Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.busy_timeout(Duration::from_millis(250))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        migrations::migrate(&mut connection, &paths, initial)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self { connection, paths })
    }

    /// Read coherent related rows and observe installation availability on this worker.
    pub fn snapshot(&self) -> Result<Snapshot> {
        // Related rows must come from one SQLite read snapshot, including when
        // another desktop instance commits between the queries below.
        let transaction = self.connection.unchecked_transaction()?;
        let version: i64 = self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        let artwork = if version >= 4 { "a.path" } else { "NULL" };
        let join = if version >= 4 {
            "LEFT JOIN local_artwork a ON a.game_id=g.id"
        } else {
            ""
        };
        let (review, cover, hero) = if version >= 5 {
            (
                "EXISTS(SELECT 1 FROM approved_reviews r WHERE r.game_id=g.id)",
                "(SELECT f.path FROM fetched_artwork f WHERE f.game_id=g.id AND f.kind='cover')",
                "(SELECT f.path FROM fetched_artwork f WHERE f.game_id=g.id AND f.kind='hero')",
            )
        } else {
            ("0", "NULL", "NULL")
        };
        let mut query = self.connection.prepare(&format!("SELECT g.id,i.id,p.id,p.revision,g.favorite,g.display_name,p.executable,p.cwd,p.runner,p.prefix,{artwork},{review},{cover},{hero} FROM games g JOIN installations i ON i.game_id=g.id JOIN profiles p ON p.installation_id=i.id {join} ORDER BY g.display_name COLLATE NOCASE,g.id"))?;
        let mut rows = query.query([])?;
        let mut entries = Vec::new();
        while let Some(row) = rows.next()? {
            let profile_id = decode_id(row.get(2)?)?;
            let mut args = self.connection.prepare(
                "SELECT value FROM arguments WHERE profile_id=?1 ORDER BY position LIMIT 1025",
            )?;
            let arguments = args
                .query_map([profile_id.as_bytes().as_slice()], |r| {
                    r.get::<_, Vec<u8>>(0)
                })?
                .map(|r| OsBytes::new(r?))
                .collect::<Result<Vec<_>>>()?;
            let mut env = self.connection.prepare(
                "SELECT key,value FROM environment WHERE profile_id=?1 ORDER BY position LIMIT 257",
            )?;
            let environment = env
                .query_map([profile_id.as_bytes().as_slice()], |r| {
                    Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
                })?
                .map(|r| {
                    let (key, value) = r?;
                    Ok(EnvDelta {
                        key: OsBytes::new(key)?,
                        value: value.map(OsBytes::new).transpose()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let profile = ProfileInput {
                name: row.get(5)?,
                executable: OsBytes::new(row.get::<_, Vec<u8>>(6)?)?,
                cwd: OsBytes::new(row.get::<_, Vec<u8>>(7)?)?,
                arguments,
                environment,
                runner: Runner::parse(&row.get::<_, String>(8)?)?,
                prefix: row
                    .get::<_, Option<Vec<u8>>>(9)?
                    .map(OsBytes::new)
                    .transpose()?,
            };
            profile.validate()?;
            let availability = availability(&profile);
            let artwork = row
                .get::<_, Option<Vec<u8>>>(10)?
                .map(OsBytes::new)
                .transpose()?;
            if artwork
                .as_ref()
                .is_some_and(|path| !path.path().is_absolute())
            {
                return Err(Error::Database(
                    "A saved artwork path is not absolute".into(),
                ));
            }
            let artwork_available = artwork.as_ref().is_some_and(|path| {
                fs::metadata(path.path())
                    .is_ok_and(|meta| meta.is_file() && meta.len() <= 32 * 1024 * 1024)
            });
            entries.push(Entry {
                game_id: decode_id(row.get(0)?)?,
                installation_id: decode_id(row.get(1)?)?,
                profile_id,
                revision: row.get(3)?,
                favorite: row.get(4)?,
                artwork,
                artwork_available,
                review_approved: row.get(11)?,
                fetched_cover: owned_artwork(row.get(12)?)?,
                fetched_hero: owned_artwork(row.get(13)?)?,
                profile,
                availability,
            });
        }
        let selected: Option<Vec<u8>> = self.connection.query_row(
            "SELECT selected_game FROM ui_state WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let (settings, warning) = match self.paths.read_settings() {
            Ok(value) => (value, None),
            Err(error) => (Settings::default(), Some(error.to_string())),
        };
        drop(rows);
        drop(query);
        transaction.commit()?;
        let (artwork, warning) = match crate::artwork::status(&self.paths) {
            Ok(status) => (status, warning),
            Err(error) => (
                crate::artwork::Status::default(),
                warning.or_else(|| Some(error.to_string())),
            ),
        };
        Ok(Snapshot {
            artwork,
            entries,
            selected: selected.map(decode_id).transpose()?,
            settings,
            warning,
        })
    }

    /// Save or clear a reference without reading, changing or deleting the external file.
    pub fn set_artwork(&mut self, game: Uuid, path: Option<&OsBytes>) -> Result<()> {
        if path.is_some_and(|value| !value.path().is_absolute()) {
            return Err(Error::Invalid(
                "Choose an absolute local artwork path.".into(),
            ));
        }
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id=?1)",
            [game.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::NotFound);
        }
        if let Some(path) = path {
            transaction.execute("INSERT INTO local_artwork(game_id,path) VALUES(?1,?2) ON CONFLICT(game_id) DO UPDATE SET path=excluded.path", params![game.as_bytes().as_slice(),path.as_bytes()])?;
        } else {
            transaction.execute(
                "DELETE FROM local_artwork WHERE game_id=?1",
                [game.as_bytes().as_slice()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// D-010: the fingerprint of the launch review last approved for this game.
    pub fn approved_review(&self, game: Uuid) -> Result<Option<Vec<u8>>> {
        Ok(self
            .connection
            .query_row(
                "SELECT fingerprint FROM approved_reviews WHERE game_id=?1",
                [game.as_bytes().as_slice()],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// D-010: record that the user approved this exact review for the game.
    pub fn approve_review(&mut self, game: Uuid, fingerprint: &[u8; 32]) -> Result<()> {
        let approved_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let changed = self.connection.execute(
            "INSERT INTO approved_reviews(game_id,fingerprint,approved_ms) SELECT ?1,?2,?3 WHERE EXISTS(SELECT 1 FROM games WHERE id=?1) ON CONFLICT(game_id) DO UPDATE SET fingerprint=excluded.fingerprint,approved_ms=excluded.approved_ms",
            params![game.as_bytes().as_slice(), fingerprint.as_slice(), approved_ms],
        )?;
        if changed == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    /// D-011: record a downloaded file; returns the file it replaced, which the
    /// caller removes after the commit.
    pub fn set_fetched_artwork(
        &mut self,
        game: Uuid,
        kind: crate::artwork::Kind,
        path: &OsBytes,
        source: &str,
    ) -> Result<Option<OsBytes>> {
        if !path.path().is_absolute() {
            return Err(Error::Invalid(
                "Downloaded artwork needs an absolute path.".into(),
            ));
        }
        let fetched_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM games WHERE id=?1)",
            [game.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::NotFound);
        }
        let previous: Option<Vec<u8>> = transaction
            .query_row(
                "SELECT path FROM fetched_artwork WHERE game_id=?1 AND kind=?2",
                params![game.as_bytes().as_slice(), kind.key()],
                |r| r.get(0),
            )
            .optional()?;
        transaction.execute(
            "INSERT INTO fetched_artwork(game_id,kind,path,source,fetched_ms) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(game_id,kind) DO UPDATE SET path=excluded.path,source=excluded.source,fetched_ms=excluded.fetched_ms",
            params![game.as_bytes().as_slice(), kind.key(), path.as_bytes(), source, fetched_ms],
        )?;
        transaction.commit()?;
        previous.map(OsBytes::new).transpose()
    }

    /// Whether a user chose this exact file as a game's own artwork.
    pub fn is_local_artwork(&self, path: &OsBytes) -> bool {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM local_artwork WHERE path=?1)",
                [path.as_bytes()],
                |r| r.get(0),
            )
            .unwrap_or(true)
    }

    /// D-011: remove downloaded files no game records any more: folders of removed
    /// games, files replaced or never recorded after a crash, and stale partial
    /// downloads. Anything a user chose as their own artwork is kept. Best effort.
    pub fn sweep_owned_artwork(&self) {
        use std::os::unix::ffi::OsStrExt;
        let root = self.paths.data.join("artwork");
        let Ok(folders) = fs::read_dir(&root) else {
            return;
        };
        let keep = |path: &Path| -> bool {
            let bytes = path.as_os_str().as_bytes();
            self.connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM fetched_artwork WHERE path=?1) OR EXISTS(SELECT 1 FROM local_artwork WHERE path=?1)",
                    [bytes],
                    |r| r.get::<_, bool>(0),
                )
                .unwrap_or(true)
        };
        for folder in folders.flatten().take(100_000) {
            let path = folder.path();
            if !folder.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let game = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| uuid::Uuid::parse_str(n).ok());
            let Some(game) = game else { continue };
            let known: bool = self
                .connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM games WHERE id=?1)",
                    [game.as_bytes().as_slice()],
                    |r| r.get(0),
                )
                .unwrap_or(true);
            let Ok(files) = fs::read_dir(&path) else {
                continue;
            };
            let files: Vec<_> = files.flatten().map(|f| f.path()).collect();
            if !known && !files.iter().any(|f| keep(f)) {
                let _ = fs::remove_dir_all(&path);
                continue;
            }
            for file in files {
                if !keep(&file) {
                    let _ = fs::remove_file(&file);
                }
            }
        }
    }

    /// D-010: a new runner selection is reviewed again for every game.
    pub fn clear_approved_reviews(&mut self) -> Result<()> {
        self.connection
            .execute("DELETE FROM approved_reviews", [])?;
        Ok(())
    }

    /// Atomically create a profile or edit the given game at its expected revision.
    /// A stale revision returns Conflict without changing any related records.
    pub fn save(&mut self, identity: Option<(Uuid, i64)>, input: &ProfileInput) -> Result<Uuid> {
        input.validate()?;
        let transaction = self
            .connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (game, installation, profile) = if let Some((game, revision)) = identity {
            let found: Option<(Vec<u8>,Vec<u8>,i64)> = transaction.query_row("SELECT i.id,p.id,p.revision FROM installations i JOIN profiles p ON p.installation_id=i.id WHERE i.game_id=?1", [game.as_bytes().as_slice()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (installation, profile, actual) = found.ok_or(Error::NotFound)?;
            if actual != revision {
                return Err(Error::Conflict);
            }
            transaction.execute(
                "UPDATE games SET display_name=?1 WHERE id=?2",
                params![input.name, game.as_bytes().as_slice()],
            )?;
            transaction.execute(
                "UPDATE profiles SET revision=revision+1 WHERE id=?1",
                [profile.as_slice()],
            )?;
            // D-010: an edited profile is reviewed again before its next UMU launch.
            transaction.execute(
                "DELETE FROM approved_reviews WHERE game_id=?1",
                [game.as_bytes().as_slice()],
            )?;
            (game, decode_id(installation)?, decode_id(profile)?)
        } else {
            let (game, installation, profile) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
            transaction.execute(
                "INSERT INTO games(id,display_name) VALUES(?1,?2)",
                params![game.as_bytes().as_slice(), input.name],
            )?;
            transaction.execute(
                "INSERT INTO installations(id,game_id) VALUES(?1,?2)",
                params![
                    installation.as_bytes().as_slice(),
                    game.as_bytes().as_slice()
                ],
            )?;
            transaction.execute("INSERT INTO profiles(id,installation_id,executable,cwd,runner,prefix) VALUES(?1,?2,?3,?4,?5,?6)", params![profile.as_bytes().as_slice(),installation.as_bytes().as_slice(),input.executable.as_bytes(),input.cwd.as_bytes(),input.runner.key(),input.prefix.as_ref().map(OsBytes::as_bytes)])?;
            (game, installation, profile)
        };
        transaction.execute("UPDATE profiles SET executable=?1,cwd=?2,runner=?3,prefix=?4 WHERE id=?5 AND installation_id=?6", params![input.executable.as_bytes(),input.cwd.as_bytes(),input.runner.key(),input.prefix.as_ref().map(OsBytes::as_bytes),profile.as_bytes().as_slice(),installation.as_bytes().as_slice()])?;
        transaction.execute(
            "DELETE FROM arguments WHERE profile_id=?1",
            [profile.as_bytes().as_slice()],
        )?;
        transaction.execute(
            "DELETE FROM environment WHERE profile_id=?1",
            [profile.as_bytes().as_slice()],
        )?;
        for (position, value) in input.arguments.iter().enumerate() {
            transaction.execute(
                "INSERT INTO arguments(profile_id,position,value) VALUES(?1,?2,?3)",
                params![
                    profile.as_bytes().as_slice(),
                    position as i64,
                    value.as_bytes()
                ],
            )?;
        }
        for (position, value) in input.environment.iter().enumerate() {
            transaction.execute(
                "INSERT INTO environment(profile_id,position,key,value) VALUES(?1,?2,?3,?4)",
                params![
                    profile.as_bytes().as_slice(),
                    position as i64,
                    value.key.as_bytes(),
                    value.value.as_ref().map(OsBytes::as_bytes)
                ],
            )?;
        }
        transaction.execute(
            "UPDATE ui_state SET selected_game=?1 WHERE id=1",
            [game.as_bytes().as_slice()],
        )?;
        transaction.commit()?;
        Ok(game)
    }

    pub fn favorite(&self, game: Uuid, value: bool) -> Result<()> {
        let count = self.connection.execute(
            "UPDATE games SET favorite=?1 WHERE id=?2",
            params![value, game.as_bytes().as_slice()],
        )?;
        if count == 0 {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub fn select(&self, game: Uuid) -> Result<()> {
        let count: i64 = self.connection.query_row(
            "SELECT count(*) FROM games WHERE id=?1",
            [game.as_bytes().as_slice()],
            |r| r.get(0),
        )?;
        if count == 0 {
            return Err(Error::NotFound);
        }
        self.connection.execute(
            "UPDATE ui_state SET selected_game=?1 WHERE id=1",
            [game.as_bytes().as_slice()],
        )?;
        Ok(())
    }

    /// Delete owned records only; never remove or rewrite the installation's files.
    pub fn remove(&self, game: Uuid) -> Result<()> {
        if self.connection.execute(
            "DELETE FROM games WHERE id=?1",
            [game.as_bytes().as_slice()],
        )? == 0
        {
            return Err(Error::NotFound);
        }
        Ok(())
    }

    pub fn preferences(&self, value: Settings) -> Result<()> {
        self.paths.write_settings(value)
    }
}

/// Downloaded files are presented only while they exist and fit the decode cap.
fn owned_artwork(path: Option<Vec<u8>>) -> Result<Option<OsBytes>> {
    let Some(path) = path.map(OsBytes::new).transpose()? else {
        return Ok(None);
    };
    Ok(fs::metadata(path.path())
        .is_ok_and(|meta| meta.is_file() && meta.len() <= 32 * 1024 * 1024)
        .then_some(path))
}

fn inspect_existing(path: &Path, paths: &Paths) -> Result<()> {
    let mut header = [0_u8; 100];
    fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|_| Error::Database("The library header is incomplete or unreadable".into()))?;
    if &header[..16] != b"SQLite format 3\0"
        || i64::from(u32::from_be_bytes([
            header[68], header[69], header[70], header[71],
        ])) != APP_ID
    {
        return Err(Error::Database(
            "This is not a recognized Rustris database".into(),
        ));
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !(1..=SCHEMA).contains(&version) {
        return Err(Error::Database(format!(
            "Unsupported library schema {version}; this build supports {SCHEMA}"
        )));
    }
    let check: String = connection.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        return Err(Error::Database(
            "The library failed its integrity check".into(),
        ));
    }
    let foreign = connection.prepare("PRAGMA foreign_key_check")?.exists([])?;
    if foreign {
        return Err(Error::Database(
            "The library has broken record relationships".into(),
        ));
    }
    // Validate the known schema/values while still read-only, before changing journal mode.
    let store = Store {
        connection,
        paths: paths.clone(),
    };
    store.snapshot()?;
    if version >= 2 {
        store.validate_sessions()?;
    }
    Ok(())
}

fn decode_id(bytes: Vec<u8>) -> Result<Uuid> {
    Uuid::from_slice(&bytes)
        .map_err(|_| Error::Database("A stored record has an invalid identity".into()))
}

fn availability(profile: &ProfileInput) -> String {
    for (name, path, is_dir) in [
        ("Executable", profile.executable.path(), false),
        ("Working directory", profile.cwd.path(), true),
    ] {
        match fs::metadata(path) {
            Ok(meta) if (is_dir && meta.is_dir()) || (!is_dir && meta.is_file()) => {}
            Ok(_) => return format!("{name}: wrong file type"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return format!("{name} missing");
            }
            Err(_) => return format!("{name} access unknown"),
        }
    }
    "Files present · launch not checked".into()
}
