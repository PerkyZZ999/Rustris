use super::*;
use crate::runners::umu::{self, PrefixIdentity, PreparedCommand, Selection};
use rusqlite::{Connection, OptionalExtension, params};

pub(super) fn insert(
    connection: &Connection,
    session: Uuid,
    command: &PreparedCommand,
) -> Result<RunnerSnapshot> {
    let snapshot = RunnerSnapshot {
        selection: command.selection.clone(),
        umu_version: command.umu_version.clone(),
        proton_version: command.proton_version.clone(),
        runtime_version: command.runtime_version.clone(),
        policy: umu::POLICY.into(),
        prefix: command.prefix.clone(),
    };
    connection.execute(
        "INSERT INTO session_runners VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            session.as_bytes().as_slice(),
            snapshot.selection.executable.as_bytes(),
            snapshot.selection.proton.as_bytes(),
            snapshot.selection.runtime.as_bytes(),
            snapshot.umu_version,
            snapshot.proton_version,
            snapshot.runtime_version,
            snapshot.policy,
            snapshot.prefix.path.as_bytes(),
            snapshot.prefix.device.to_be_bytes().as_slice(),
            snapshot.prefix.inode.to_be_bytes().as_slice()
        ],
    )?;
    Ok(snapshot)
}
pub(super) fn load(connection: &Connection, session: Uuid) -> Result<Option<RunnerSnapshot>> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 3 {
        return Ok(None);
    }
    let raw=connection.query_row("SELECT executable,proton,runtime,umu_version,proton_version,runtime_version,policy,prefix_path,prefix_device,prefix_inode FROM session_runners WHERE session_id=?1",[session.as_bytes().as_slice()],|r|Ok((r.get::<_,Vec<u8>>(0)?,r.get::<_,Vec<u8>>(1)?,r.get::<_,Vec<u8>>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,Vec<u8>>(7)?,r.get::<_,Vec<u8>>(8)?,r.get::<_,Vec<u8>>(9)?))).optional()?;
    raw.map(
        |(
            executable,
            proton,
            runtime,
            umu_version,
            proton_version,
            runtime_version,
            policy,
            prefix,
            device,
            inode,
        )| {
            let selection = Selection {
                executable: OsBytes::new(executable)?,
                proton: OsBytes::new(proton)?,
                runtime: OsBytes::new(runtime)?,
            };
            let prefix = PrefixIdentity {
                path: OsBytes::new(prefix)?,
                device: number(device)?,
                inode: number(inode)?,
            };
            if [
                &selection.executable,
                &selection.proton,
                &selection.runtime,
                &prefix.path,
            ]
            .iter()
            .any(|p| !p.path().is_absolute())
                || policy != umu::POLICY
                || !umu_version.starts_with("umu-launcher version ")
                || !umu::snapshot_supported(
                    &proton_version,
                    &runtime_version,
                    &selection.runtime.path(),
                )
            {
                return Err(Error::Database(
                    "The saved runner snapshot is invalid or unsupported".into(),
                ));
            }
            Ok(RunnerSnapshot {
                selection,
                umu_version,
                proton_version,
                runtime_version,
                policy,
                prefix,
            })
        },
    )
    .transpose()
}
fn number(bytes: Vec<u8>) -> Result<u64> {
    let bytes: [u8; 8] = bytes
        .try_into()
        .map_err(|_| Error::Database("A saved runner directory identity is invalid".into()))?;
    Ok(u64::from_be_bytes(bytes))
}
