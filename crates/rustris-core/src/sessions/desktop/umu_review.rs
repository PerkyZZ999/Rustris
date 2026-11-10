use super::*;
use crate::{domain::Runner, runners::umu};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Clone, Serialize)]
pub struct Review {
    pub ticket: Uuid,
    pub game: Uuid,
    pub name: String,
    pub ready: bool,
    pub tool: String,
    pub proton: String,
    pub runtime: String,
    pub game_executable: String,
    pub cwd: String,
    pub prefix: String,
    pub effective_prefix: Option<String>,
    pub umu_version: Option<String>,
    pub proton_version: Option<String>,
    pub runtime_version: Option<String>,
    pub arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub preflight: native::Preflight,
    /// D-010: the user approved this exact review before; Play may skip the dialog.
    pub approved: bool,
    /// D-012: false when this UMU, Proton and runtime set has not been tested.
    pub validated: bool,
}
pub(super) struct Pending {
    pub game: Uuid,
    pub revision: i64,
    pub plan: Option<umu::PreparedCommand>,
    pub view: Review,
    /// Present only when preparation produced a launchable plan.
    pub fingerprint: Option<[u8; 32]>,
}

/// D-010: a digest of everything the review shows that could change what is
/// launched. Desktop variables inherited from the session (display, bus and so
/// on) are left out so a new login does not ask again; the profile's own
/// environment changes and the runner-owned controls are included.
pub(super) fn fingerprint(
    selection: &umu::Selection,
    profile: &crate::domain::ProfileInput,
    plan: &umu::PreparedCommand,
) -> [u8; 32] {
    fn field(hash: &mut Sha256, tag: &[u8], value: &[u8]) {
        hash.update(tag);
        hash.update((value.len() as u64).to_le_bytes());
        hash.update(value);
    }
    let mut hash = Sha256::new();
    field(&mut hash, b"version", b"rustris-review-v1");
    field(&mut hash, b"tool", selection.executable.as_bytes());
    field(&mut hash, b"proton", selection.proton.as_bytes());
    field(&mut hash, b"runtime", selection.runtime.as_bytes());
    field(&mut hash, b"umu-version", plan.umu_version().as_bytes());
    field(
        &mut hash,
        b"proton-version",
        plan.proton_version().as_bytes(),
    );
    field(
        &mut hash,
        b"runtime-version",
        plan.runtime_version().as_bytes(),
    );
    let prefix = plan.prefix();
    field(&mut hash, b"prefix", prefix.path.as_bytes());
    field(&mut hash, b"prefix-device", &prefix.device.to_le_bytes());
    field(&mut hash, b"prefix-inode", &prefix.inode.to_le_bytes());
    field(&mut hash, b"game", profile.executable.as_bytes());
    field(&mut hash, b"cwd", plan.cwd().as_bytes());
    field(&mut hash, b"executable", plan.executable().as_bytes());
    for argument in plan.arguments() {
        field(&mut hash, b"argument", argument.as_bytes());
    }
    for delta in &profile.environment {
        field(&mut hash, b"env-key", delta.key.as_bytes());
        match &delta.value {
            Some(value) => field(&mut hash, b"env-set", value.as_bytes()),
            None => field(&mut hash, b"env-unset", b""),
        }
    }
    for (key, value) in plan.environment() {
        if umu::runner_owned(key.as_bytes()) {
            field(&mut hash, b"runner-key", key.as_bytes());
            field(&mut hash, b"runner-value", value.as_bytes());
        }
    }
    hash.finalize().into()
}
impl Service {
    pub(super) fn preview_umu(&mut self, game: Uuid, revision: i64) -> Result<Pending> {
        let selection = self.paths.read_runner_selection()?.ok_or_else(|| {
            Error::Invalid(
                "Choose your prepared UMU, Proton and runtime paths in Runner settings first."
                    .into(),
            )
        })?;
        let entry = self
            .storage()?
            .snapshot()?
            .entries
            .into_iter()
            .find(|e| e.game_id == game)
            .ok_or(Error::NotFound)?;
        if entry.revision != revision {
            return Err(Error::Conflict);
        }
        if entry.profile.runner != Runner::Umu {
            return Err(Error::Invalid(
                "This profile is not configured for UMU.".into(),
            ));
        }
        let prepared = umu::inspect(&selection, &entry.profile, &BaseEnvironment::capture()?)?;
        let plan = prepared.command;
        let fingerprint = plan
            .as_ref()
            .map(|plan| fingerprint(&selection, &entry.profile, plan));
        let approved = match fingerprint {
            Some(value) => self
                .storage()?
                .approved_review(game)?
                .is_some_and(|stored| stored.as_slice() == value.as_slice()),
            None => false,
        };
        let view = Review {
            ticket: Uuid::new_v4(),
            game,
            name: entry.profile.name.clone(),
            ready: plan.is_some(),
            tool: selection.executable.label(),
            proton: selection.proton.label(),
            runtime: selection.runtime.label(),
            game_executable: entry.profile.executable.label(),
            cwd: entry.profile.cwd.label(),
            prefix: entry
                .profile
                .prefix
                .as_ref()
                .map_or_else(String::new, |p| p.label()),
            effective_prefix: plan.as_ref().map(|p| p.prefix().path.label()),
            umu_version: plan.as_ref().map(|p| p.umu_version().into()),
            proton_version: plan.as_ref().map(|p| p.proton_version().into()),
            runtime_version: plan.as_ref().map(|p| p.runtime_version().into()),
            arguments: plan.as_ref().map_or_else(Vec::new, |p| {
                p.arguments().iter().skip(1).map(|a| a.label()).collect()
            }),
            environment: plan.as_ref().map_or_else(Vec::new, |p| {
                p.environment()
                    .iter()
                    .map(|(k, v)| (k.label(), v.label()))
                    .collect()
            }),
            preflight: prepared.evidence,
            approved,
            validated: plan.as_ref().is_some_and(umu::PreparedCommand::validated),
        };
        Ok(Pending {
            game,
            revision,
            plan,
            view,
            fingerprint,
        })
    }
}
