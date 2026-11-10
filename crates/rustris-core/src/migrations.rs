use crate::{Result, error::io, paths::Paths};
use rusqlite::{
    Connection, OpenFlags, TransactionBehavior,
    backup::{Backup, StepResult},
};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::OpenOptionsExt,
    time::{Duration, Instant},
};

pub(crate) const APP_ID: i64 = 0x5255_5354;
pub(crate) const SCHEMA: i64 = 5;

#[cfg(test)]
mod tests;

pub(crate) fn migrate(connection: &mut Connection, paths: &Paths, initial: bool) -> Result<()> {
    if !initial
        && connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?
            == SCHEMA
    {
        return Ok(());
    }
    // Both workers open the library at startup. While one upgrades it (backup,
    // integrity check, fsync), the other must wait rather than fail after the
    // normal 250 ms busy timeout; it then sees the new version and does nothing.
    connection.busy_timeout(Duration::from_secs(60))?;
    let result = migrate_locked(connection, paths, initial);
    connection.busy_timeout(Duration::from_millis(250))?;
    result
}

fn migrate_locked(connection: &mut Connection, paths: &Paths, initial: bool) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = transaction.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if initial && version == 0 {
        transaction.execute_batch(include_str!("schema.sql"))?;
        transaction.pragma_update(None, "application_id", APP_ID)?;
    } else if (1..SCHEMA).contains(&version) {
        // The reserved write lock prevents concurrent writers while a separate
        // read connection backs up all committed pages, including the live WAL.
        backup_before_migration(paths, version)?;
        #[cfg(test)]
        tests::checkpoint(paths, "backup-published");
    } else if version == SCHEMA {
        transaction.commit()?;
        return Ok(());
    } else {
        return Err(crate::Error::Database(format!(
            "Unsupported library schema {version}"
        )));
    }
    if version < 2 {
        transaction.execute_batch(include_str!("schema002.sql"))?;
    }
    if version < 3 {
        transaction.execute_batch(include_str!("schema003.sql"))?;
    }
    if version < 4 {
        transaction.execute_batch(include_str!("schema004.sql"))?;
    }
    transaction.execute_batch(include_str!("schema005.sql"))?;
    transaction.pragma_update(None, "user_version", SCHEMA)?;
    #[cfg(test)]
    tests::checkpoint(paths, "ddl-staged");
    transaction.commit()?;
    #[cfg(test)]
    tests::checkpoint(paths, "commit-completed");
    Ok(())
}

fn backup_before_migration(paths: &Paths, version: i64) -> Result<()> {
    let identity = uuid::Uuid::now_v7();
    let path = paths
        .data
        .join(format!(".schema-{version}-backup-{identity}.incomplete"));
    let completed = paths
        .data
        .join(format!("schema-{version}-backup-{identity}.sqlite3"));
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&path)
        .map_err(|e| io("create the migration backup", e))?;
    let result = write_backup(paths, &path, &completed);
    if result.is_err() {
        // A failed attempt leaves nothing behind that could be mistaken for a backup.
        let _ = fs::remove_file(&path);
    }
    result
}

fn write_backup(paths: &Paths, path: &std::path::Path, completed: &std::path::Path) -> Result<()> {
    let source = Connection::open_with_flags(paths.database(), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut destination = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    {
        let backup = Backup::new(&source, &mut destination)?;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match backup.step(128)? {
                StepResult::Done => break,
                StepResult::More if Instant::now() < deadline => {}
                StepResult::Busy | StepResult::Locked => {
                    return Err(crate::Error::Database(
                        "The library is busy; retry after other writers finish".into(),
                    ));
                }
                _ => {
                    return Err(crate::Error::Database(
                        "The migration backup did not complete within 30 seconds".into(),
                    ));
                }
            }
        }
    }
    let result: String = destination.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if result != "ok" {
        return Err(crate::Error::Database(
            "The migration backup failed its integrity check".into(),
        ));
    }
    destination.close().map_err(|(_, error)| error)?;
    #[cfg(test)]
    tests::backup_fault("sync")?;
    fs::File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|e| io("sync the migration backup", e))?;
    #[cfg(test)]
    tests::backup_fault("publish")?;
    fs::hard_link(path, completed).map_err(|e| io("publish the verified migration backup", e))?;
    fs::remove_file(path).map_err(|e| io("finish the verified migration backup", e))?;
    #[cfg(test)]
    tests::backup_fault("directory-sync")?;
    fs::File::open(&paths.data)
        .and_then(|directory| directory.sync_all())
        .map_err(|e| io("sync the migration backup directory", e))?;
    Ok(())
}
