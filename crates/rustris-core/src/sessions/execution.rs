use super::{
    LaunchSnapshot, SessionState,
    native::{Finding, Observation, Preflight},
};
use crate::{Error, Result, domain::Runner, runners::umu, store::Store};
use std::process::Command;
use uuid::Uuid;

pub(super) struct Resolved {
    pub report: Preflight,
    pub command: Option<Command>,
}
pub(super) fn resolve(snapshot: &LaunchSnapshot) -> Result<Resolved> {
    let profile = &snapshot.source.profile;
    let (mut report, plan) = if profile.runner == Runner::Native {
        (Preflight::inspect(snapshot)?, None)
    } else {
        let frozen=snapshot.runtime.as_ref().ok_or_else(||Error::Invalid("This UMU session has no frozen runner provenance. Prepare a new session; no game was started.".into()))?;
        let base = super::BaseEnvironment::from_pairs(
            snapshot
                .environment
                .iter()
                .map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())),
        )?;
        let prepared = umu::inspect(&frozen.selection, profile, &base)?;
        (prepared.evidence, prepared.command)
    };
    if let Some(plan) = &plan {
        let frozen = snapshot
            .runtime
            .as_ref()
            .ok_or_else(|| Error::Invalid("Runner provenance is unavailable.".into()))?;
        if frozen.policy != umu::POLICY
            || frozen.umu_version != plan.umu_version
            || frozen.proton_version != plan.proton_version
            || frozen.runtime_version != plan.runtime_version
            || frozen.prefix.path != plan.prefix.path
            || frozen.prefix.device != plan.prefix.device
            || frozen.prefix.inode != plan.prefix.inode
            || snapshot.environment != plan.environment
        {
            reject(
                &mut report,
                "frozen_runner",
                "The selected runner, environment or prefix changed after preparation. Prepare and review it again; no game was started.",
            )?;
        }
    }
    if !report.findings.is_empty() {
        return Ok(Resolved {
            report,
            command: None,
        });
    }
    if profile.runner == Runner::Umu && plan.is_none() {
        return Err(Error::Invalid(
            "The UMU command could not be prepared; no game was started.".into(),
        ));
    }
    let executable = plan.as_ref().map_or(&profile.executable, |p| &p.executable);
    let arguments = plan
        .as_ref()
        .map_or(profile.arguments.as_slice(), |p| p.arguments.as_slice());
    let mut command = Command::new(executable.path());
    command
        .current_dir(profile.cwd.path())
        .args(arguments.iter().map(|v| v.path().into_os_string()))
        .env_clear()
        .envs(
            snapshot
                .environment
                .iter()
                .map(|(k, v)| (k.path().into_os_string(), v.path().into_os_string())),
        );
    Ok(Resolved {
        report,
        command: Some(command),
    })
}
pub(super) fn verify_spawn_scope(store: &Store, session: &super::Session) -> Result<()> {
    let (state, generation) = store.session_head(session.id)?;
    if state != SessionState::Preparing || generation != session.generation {
        return Err(Error::SessionChanged);
    }
    let reserved: bool = store.connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM session_reservations WHERE session_id=?1 AND profile_id=?2)",
        rusqlite::params![
            session.id.as_bytes().as_slice(),
            session.snapshot.source.profile_id.as_bytes().as_slice()
        ],
        |r| r.get(0),
    )?;
    if !reserved {
        return Err(Error::SessionBusy);
    }
    if let Some(runtime) = &session.snapshot.runtime {
        let prefix = session
            .snapshot
            .source
            .profile
            .prefix
            .as_ref()
            .ok_or_else(|| Error::Invalid("The frozen prefix is missing.".into()))?;
        let current = umu::existing_prefix(&prefix.path())?;
        if current.path != runtime.prefix.path
            || current.device != runtime.prefix.device
            || current.inode != runtime.prefix.inode
        {
            return Err(Error::Invalid(
                "The frozen prefix changed immediately before spawn. No game was started.".into(),
            ));
        }
        // Re-read what the selected Proton writes into a prefix, as preparation did.
        let marker = umu::prefix_marker(&runtime.selection.proton.path())?;
        umu::verify_prefix_version(&current.path.path(), &marker)?;
    }
    Ok(())
}
fn reject(report: &mut Preflight, resource: &str, message: &str) -> Result<()> {
    let id = Uuid::now_v7();
    report.observations.push(Observation {
        id,
        resource: resource.into(),
        source: "frozen snapshot comparison".into(),
        observed_ms: super::now_ms()?,
        outcome: "unsupported".into(),
        errno: None,
        device: None,
        inode: None,
        size: None,
        modified_seconds: None,
        modified_nanoseconds: None,
    });
    report.findings.push(Finding {
        rule: format!("umu.{resource}.changed"),
        evidence: id,
        message: message.into(),
    });
    Ok(())
}
