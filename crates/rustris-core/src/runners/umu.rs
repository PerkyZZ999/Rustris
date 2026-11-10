mod evidence;
mod probe;
mod settings;
use crate::{
    Error, Result,
    bytes::OsBytes,
    domain::{ProfileInput, Runner},
    sessions::{BaseEnvironment, native::Preflight},
};
use evidence::{Kind, check, observe, read_text};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::Path,
    process::Command,
};

pub const POLICY: &str = "umu-prepared-v1";
pub const UMU_VERSION: &str = "1.4.3";
pub const PROTON_VERSION: &str = "UMU-Proton-10.0-4";
pub const RUNTIME_VERSION: &str = "3.0.20260805.254768";
pub const GE_PROTON_VERSION: &str = "GE-Proton11-5";
pub const STEAMRT4_VERSION: &str = "4.0.20261021.265997";

/// The Steam Linux Runtime layouts UMU prepares under `umu/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeKind {
    Sniper,
    SteamRt4,
}
impl RuntimeKind {
    fn from_directory(runtime: &Path) -> Option<Self> {
        if runtime.parent().and_then(Path::file_name) != Some(std::ffi::OsStr::new("umu")) {
            return None;
        }
        match runtime.file_name().and_then(std::ffi::OsStr::to_str)? {
            "steamrt3" => Some(Self::Sniper),
            "steamrt4" => Some(Self::SteamRt4),
            _ => None,
        }
    }
    fn runtime_name(self) -> &'static str {
        match self {
            Self::Sniper => "sniper",
            Self::SteamRt4 => "steamrt4",
        }
    }
    /// The Steam app ID a Proton build names in `require_tool_appid`.
    fn app_id(self) -> &'static str {
        match self {
            Self::Sniper => "1628350",
            Self::SteamRt4 => "4183110",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Sniper => "Steam Linux Runtime 3.0 (sniper)",
            Self::SteamRt4 => "Steam Linux Runtime 4.0 (steamrt4)",
        }
    }
}

/// A Proton build and runtime as selected. D-012: any detected combination may
/// be used; `validated` is true only for the exact pairs Rustris was tested with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunnerPair {
    runtime: RuntimeKind,
    runtime_version: String,
    /// What Proton writes into a prefix's `version` file: the
    /// `CURRENT_PREFIX_VERSION` from its `proton` script. For GE-Proton up to 11-7
    /// and UMU-Proton that equals the build name; for most other builds it does not
    /// (GE-Proton11-12 writes `11.0-100`, Steam's Proton 10.0 `10.1000-105`).
    marker: String,
    pub(crate) validated: bool,
}
impl RunnerPair {
    fn new(
        proton: &str,
        marker: String,
        runtime: RuntimeKind,
        runtime_version: &str,
    ) -> Option<Self> {
        let fields: Vec<_> = proton.split_whitespace().collect();
        if fields.is_empty()
            || runtime_version.is_empty()
            || marker.is_empty()
            || marker.len() > 128
        {
            return None;
        }
        let validated = matches!(
            (fields.as_slice(), runtime, runtime_version),
            ([_, PROTON_VERSION], RuntimeKind::Sniper, RUNTIME_VERSION)
                | (
                    ["1786437966", GE_PROTON_VERSION],
                    RuntimeKind::SteamRt4,
                    STEAMRT4_VERSION
                )
        );
        Some(Self {
            runtime,
            runtime_version: runtime_version.to_owned(),
            marker,
            validated,
        })
    }
    fn marker(&self) -> &str {
        &self.marker
    }
    fn runtime_name(&self) -> &'static str {
        self.runtime.runtime_name()
    }
    fn runtime_version(&self) -> &str {
        &self.runtime_version
    }
}
/// Whether a saved session snapshot names a usable Proton and runtime layout.
pub(crate) fn snapshot_supported(proton: &str, runtime: &str, runtime_path: &Path) -> bool {
    RuntimeKind::from_directory(runtime_path).is_some()
        && !proton
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .is_empty()
        && !runtime.is_empty()
}

const MAX_PROTON_SCRIPT: u64 = 4 * 1024 * 1024;
/// Read the prefix version a Proton build writes: its single
/// `CURRENT_PREFIX_VERSION="…"` assignment. Missing or ambiguous values fail closed.
pub(crate) fn prefix_marker(proton: &Path) -> Result<String> {
    let mut text = String::new();
    fs::File::open(proton.join("proton"))
        .and_then(|file| file.take(MAX_PROTON_SCRIPT + 1).read_to_string(&mut text))
        .map_err(|e| crate::error::io("read the selected Proton script", e))?;
    if text.len() as u64 > MAX_PROTON_SCRIPT {
        return Err(Error::Invalid(
            "The selected Proton script is unexpectedly large.".into(),
        ));
    }
    let values: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("CURRENT_PREFIX_VERSION="))
        .map(|value| value.trim().trim_matches(['"', '\'']))
        .collect();
    match values.as_slice() {
        [value] if !value.is_empty() && value.len() <= 128 => Ok((*value).to_owned()),
        _ => Err(Error::Invalid(
            "The selected Proton build does not state which prefix version it writes, so a prefix's compatibility cannot be checked. No game was started.".into(),
        )),
    }
}
const RESERVED: &[&[u8]] = &[
    b"PROTONPATH",
    b"WINEPREFIX",
    b"GAMEID",
    b"STORE",
    b"PROTON_VERB",
    b"PROTONFIXES_DISABLE",
    b"UMU_RUNTIME_UPDATE",
    b"UMU_NO_PROTON",
    b"UMU_NO_RUNTIME",
    b"RUNTIMEPATH",
    b"UMU_FOLDERS_PATH",
    b"LD_PRELOAD",
    b"LD_AUDIT",
    b"LD_LIBRARY_PATH",
    b"PYTHONPATH",
    b"PYTHONHOME",
    b"PROTON_LOG",
    b"PROTON_LOG_DIR",
];

#[derive(Clone, Debug)]
pub struct Selection {
    pub executable: OsBytes,
    pub proton: OsBytes,
    pub runtime: OsBytes,
}
#[derive(Clone, Debug)]
pub struct PrefixIdentity {
    pub path: OsBytes,
    pub device: u64,
    pub inode: u64,
}
#[derive(Clone, Debug)]
pub struct PreparedCommand {
    pub(crate) executable: OsBytes,
    pub(crate) arguments: Vec<OsBytes>,
    pub(crate) cwd: OsBytes,
    pub(crate) environment: Vec<(OsBytes, OsBytes)>,
    pub(crate) prefix: PrefixIdentity,
    pub(crate) selection: Selection,
    pub(crate) umu_version: String,
    pub(crate) proton_version: String,
    pub(crate) runtime_version: String,
    /// D-012: true only for the exact UMU, Proton and runtime set Rustris tested.
    pub(crate) validated: bool,
    source: ProfileInput,
}
pub struct Preparation {
    pub evidence: Preflight,
    pub command: Option<PreparedCommand>,
}
impl PreparedCommand {
    pub fn executable(&self) -> &OsBytes {
        &self.executable
    }
    pub fn arguments(&self) -> &[OsBytes] {
        &self.arguments
    }
    pub fn cwd(&self) -> &OsBytes {
        &self.cwd
    }
    pub fn environment(&self) -> &[(OsBytes, OsBytes)] {
        &self.environment
    }
    pub fn prefix(&self) -> &PrefixIdentity {
        &self.prefix
    }
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    pub fn umu_version(&self) -> &str {
        &self.umu_version
    }
    pub fn proton_version(&self) -> &str {
        &self.proton_version
    }
    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }
    pub fn validated(&self) -> bool {
        self.validated
    }
    pub(crate) fn validate_source(&self, profile: &ProfileInput) -> Result<()> {
        if &self.source != profile {
            return Err(Error::Conflict);
        }
        Ok(())
    }
}

struct Versions {
    umu: String,
    proton: String,
    pair: RunnerPair,
}

/// Collect and construct only. No prerequisite or prefix is created or repaired.
/// Invoke on a worker; this does not enable the UMU session adapter or launch a game.
pub fn inspect(
    selection: &Selection,
    profile: &ProfileInput,
    base: &BaseEnvironment,
) -> Result<Preparation> {
    profile.validate()?;
    if profile.runner != Runner::Umu {
        return Err(Error::Invalid(
            "A UMU profile is required for this capability.".into(),
        ));
    }
    for value in [&selection.executable, &selection.proton, &selection.runtime] {
        if !value.path().is_absolute() {
            return Err(Error::Invalid(
                "Runner selection paths must be absolute.".into(),
            ));
        }
    }
    if profile
        .environment
        .iter()
        .any(|row| reserved(row.key.as_bytes()))
    {
        return Err(Error::Invalid("This profile overrides a runner-owned UMU, loader, Python or debug-log control. Remove that environment delta before launch.".into()));
    }
    // Validate the selected environment before invoking even a short version probe.
    let environment = base.effective(profile)?;
    let prefix = profile
        .prefix
        .as_ref()
        .ok_or_else(|| Error::Invalid("Select an existing flat Wine prefix.".into()))?
        .path();
    let mut evidence = Preflight {
        observations: Vec::new(),
        findings: Vec::new(),
    };
    prerequisites(&mut evidence, selection, profile, &prefix)?;
    let identity = prefix_identity(&mut evidence, &prefix)?;
    let mut selected = None;
    if evidence.findings.is_empty() {
        check(
            &mut evidence,
            "version_pair",
            "bounded installed version and manifest reads",
            || {
                selected = Some(inspect_pair(selection)?);
                Ok(())
            },
        )?;
    }
    if let (Some(identity), Some((pair, _))) = (&identity, &selected) {
        let platform = selection.runtime.path().join(format!(
            "{}_platform_{}",
            pair.runtime_name(),
            pair.runtime_version()
        ));
        observe(
            &mut evidence,
            "runtime_platform",
            &platform,
            Kind::Directory,
        )?;
        observe(
            &mut evidence,
            "runtime_platform_files",
            &platform.join("files"),
            Kind::Directory,
        )?;
        check(
            &mut evidence,
            "prefix_version",
            "bounded Proton prefix version marker",
            || verify_prefix_version(&identity.path.path(), pair.marker()),
        )?;
    }
    let mut versions = None;
    if evidence.findings.is_empty() {
        check(
            &mut evidence,
            "umu_version",
            "bounded installed UMU version query",
            || {
                let (pair, proton) = selected.as_ref().ok_or_else(|| {
                    Error::Invalid("The selected runner pair is unavailable.".into())
                })?;
                versions = Some(verify_versions(
                    selection,
                    profile,
                    &environment,
                    pair.clone(),
                    proton,
                )?);
                Ok(())
            },
        )?;
    }
    let command = match (identity, versions) {
        (Some(prefix), Some(versions)) if evidence.findings.is_empty() => Some(construct(
            selection,
            profile,
            environment,
            prefix,
            versions,
        )?),
        _ => None,
    };
    Ok(Preparation { evidence, command })
}
pub(crate) fn verify_prefix_version(prefix: &Path, expected: &str) -> Result<()> {
    let marker = prefix.join("version");
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags((rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32)
        .open(&marker);
    let file = match file {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if only_umu_skeleton(prefix)? {
                return Ok(());
            }
            return Err(Error::Invalid("This prefix has files but no Proton version marker, so the Proton that made it is unknown. A first launch may have been interrupted, or another tool created it. No game was started.".into()));
        }
        Err(error) => return Err(crate::error::io("read the prefix version marker", error)),
    };
    if !file
        .metadata()
        .map_err(|e| crate::error::io("inspect the prefix version marker", e))?
        .is_file()
    {
        return Err(Error::Invalid(
            "The Proton prefix version marker is not a regular file.".into(),
        ));
    }
    let mut contents = Vec::new();
    file.take(129)
        .read_to_end(&mut contents)
        .map_err(|e| crate::error::io("read the prefix version marker", e))?;
    if contents.len() > 128 || std::str::from_utf8(&contents).ok().map(str::trim) != Some(expected)
    {
        return Err(Error::Invalid("The prefix Proton version differs from the selected runner or is unreadable. Switching versions can change the prefix; no game was started.".into()));
    }
    Ok(())
}
/// UMU prepares a prefix before Proton writes its `version` marker at the end of
/// its own setup. A first launch stopped in between leaves only this skeleton,
/// which is still safe to hand to any Proton build.
fn only_umu_skeleton(prefix: &Path) -> Result<bool> {
    let list = |dir: &Path| -> Result<Vec<std::ffi::OsString>> {
        fs::read_dir(dir)
            .map_err(|e| crate::error::io("inspect the prefix contents", e))?
            .take(64)
            .map(|entry| {
                entry
                    .map(|entry| entry.file_name())
                    .map_err(|e| crate::error::io("inspect the prefix contents", e))
            })
            .collect()
    };
    let user = std::env::var_os("USER");
    for name in list(prefix)? {
        match name.to_str() {
            Some("pfx" | "pfx.lock" | "shadercache" | "gstreamer-1.0" | "tracked_files") => {}
            Some("drive_c") => {
                for entry in list(&prefix.join("drive_c"))? {
                    if entry != "users" {
                        return Ok(false);
                    }
                    for account in list(&prefix.join("drive_c/users"))? {
                        if account != "steamuser" && Some(&account) != user.as_ref() {
                            return Ok(false);
                        }
                    }
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}
fn prerequisites(
    evidence: &mut Preflight,
    selection: &Selection,
    profile: &ProfileInput,
    prefix: &Path,
) -> Result<()> {
    let proton = selection.proton.path();
    let runtime = selection.runtime.path();
    for (resource, path, kind) in [
        ("umu", selection.executable.path(), Kind::Executable),
        ("proton", proton.clone(), Kind::Directory),
        ("runtime", runtime.clone(), Kind::Directory),
        ("prefix", prefix.into(), Kind::Directory),
        ("working_directory", profile.cwd.path(), Kind::Directory),
        ("game", profile.executable.path(), Kind::File),
        ("proton_entry", proton.join("proton"), Kind::Executable),
        (
            "runtime_entry",
            runtime.join("_v2-entry-point"),
            Kind::Executable,
        ),
        (
            "pressure_vessel",
            runtime.join("pressure-vessel/bin/pressure-vessel-wrap"),
            Kind::Executable,
        ),
        ("proton_version", proton.join("version"), Kind::File),
        ("toolmanifest", proton.join("toolmanifest.vdf"), Kind::File),
        ("runtime_version", runtime.join("VERSIONS.txt"), Kind::File),
        ("runtime_marker", runtime.join(".installed.ok"), Kind::File),
    ] {
        observe(evidence, resource, &path, kind)?;
    }
    check(
        evidence,
        "custom_settings",
        "filesystem symlink_metadata",
        || {
            match fs::symlink_metadata(proton.join("user_settings.py")) {
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>Ok(()),
        Ok(_)=>Err(Error::Invalid("Custom Proton user_settings.py requires a separate capability review; it is not executed.".into())),
        Err(e)=>Err(crate::error::io("inspect custom Proton settings",e)),
    }
        },
    )?;
    check(
        evidence,
        "runtime_lookup",
        "selected UMU runtime layout",
        || {
            runtime_root(&runtime)?;
            Ok(())
        },
    )?;
    if let Some(parent) = runtime.parent() {
        observe(
            evidence,
            "umu_shim",
            &parent.join("umu-shim"),
            Kind::Executable,
        )?;
    }
    check(evidence, "game_kind", "saved executable path", || {
        if !profile
            .executable
            .path()
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        {
            return Err(Error::Invalid("Select an existing Windows .exe; UMU verbs and automatic installers are not enabled.".into()));
        }
        Ok(())
    })?;
    Ok(())
}
fn prefix_identity(evidence: &mut Preflight, prefix: &Path) -> Result<Option<PrefixIdentity>> {
    let mut identity = None;
    check(
        evidence,
        "prefix_layout",
        "canonical path and physical directory identity",
        || {
            identity = Some(existing_prefix(prefix)?);
            Ok(())
        },
    )?;
    Ok(identity)
}
pub(crate) fn existing_prefix(prefix: &Path) -> Result<PrefixIdentity> {
    let path =
        fs::canonicalize(prefix).map_err(|e| crate::error::io("resolve the selected prefix", e))?;
    let metadata =
        fs::metadata(&path).map_err(|e| crate::error::io("inspect the effective prefix", e))?;
    if !metadata.is_dir() {
        return Err(Error::Invalid(
            "The effective prefix is not a directory.".into(),
        ));
    }
    match fs::symlink_metadata(path.join("pfx")) {
        Ok(_) => {
            let nested = fs::canonicalize(path.join("pfx"))
                .map_err(|e| crate::error::io("resolve the inner pfx", e))?;
            if nested != path {
                return Err(Error::Invalid("Select the flat Wine prefix itself; a different nested pfx is unsupported. No repair was performed.".into()));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(crate::error::io("inspect the prefix layout", e)),
    }
    Ok(PrefixIdentity {
        path: path_bytes(&path)?,
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

fn inspect_pair(selection: &Selection) -> Result<(RunnerPair, String)> {
    let proton_text = read_text(&selection.proton.path().join("version"))?;
    let manifest = read_text(&selection.proton.path().join("toolmanifest.vdf"))?;
    let runtime_text = read_text(&selection.runtime.path().join("VERSIONS.txt"))?;
    let runtime = RuntimeKind::from_directory(&selection.runtime.path()).ok_or_else(|| {
        Error::Invalid("The prepared runtime must use a umu/steamrt3 or umu/steamrt4 layout; no directory was moved.".into())
    })?;
    let runtime_version = runtime_text
        .lines()
        .find_map(|line| {
            let mut cells = line.split('\t');
            (cells.next() == Some(runtime.runtime_name()))
                .then(|| cells.next().unwrap_or_default().trim().to_owned())
        })
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            Error::Invalid("The selected runtime's VERSIONS.txt does not name its version.".into())
        })?;
    // Proton names the runtime it was built for; a different one cannot run it.
    let required: Vec<_> = manifest
        .lines()
        .filter(|line| line.contains("\"require_tool_appid\""))
        .filter_map(|line| {
            line.split_whitespace()
                .nth(1)
                .map(|id| id.trim_matches('"').to_owned())
        })
        .collect();
    if required.len() != 1 || required[0] != runtime.app_id() {
        let wanted = match required.first().map(String::as_str) {
            Some("1628350") => RuntimeKind::Sniper.label(),
            Some("4183110") => RuntimeKind::SteamRt4.label(),
            _ => "a runtime Rustris does not recognise",
        };
        return Err(Error::Invalid(format!(
            "This Proton build needs {wanted}, but {} is selected. Choose the matching runtime.",
            runtime.label()
        )));
    }
    let marker = prefix_marker(&selection.proton.path())?;
    let pair =
        RunnerPair::new(&proton_text, marker, runtime, &runtime_version).ok_or_else(|| {
            Error::Invalid("The selected Proton build has no readable version.".into())
        })?;
    Ok((pair, proton_text.trim().into()))
}
fn verify_versions(
    selection: &Selection,
    profile: &ProfileInput,
    environment: &[(OsBytes, OsBytes)],
    pair: RunnerPair,
    proton: &str,
) -> Result<Versions> {
    let mut command = Command::new(selection.executable.path());
    command
        .env_clear()
        .current_dir(profile.cwd.path())
        .env("PYTHONDONTWRITEBYTECODE", "1");
    for (key, value) in environment {
        command.env(key.path().as_os_str(), value.path().as_os_str());
    }
    let umu = probe::version(&mut command)?;
    if !umu.starts_with("umu-launcher version ") {
        return Err(Error::Invalid(
            "The selected program did not identify itself as umu-launcher.".into(),
        ));
    }
    // D-012: other UMU releases are allowed but not validated.
    let mut pair = pair;
    pair.validated &= umu.starts_with(&format!("umu-launcher version {UMU_VERSION} ("));
    Ok(Versions {
        umu: umu.trim().into(),
        proton: proton.into(),
        pair,
    })
}
fn construct(
    selection: &Selection,
    profile: &ProfileInput,
    environment: Vec<(OsBytes, OsBytes)>,
    prefix: PrefixIdentity,
    versions: Versions,
) -> Result<PreparedCommand> {
    let mut environment: BTreeMap<Vec<u8>, OsBytes> = environment
        .into_iter()
        .map(|(k, v)| (k.as_bytes().to_vec(), v))
        .collect();
    let runtime = selection.runtime.path();
    let root = runtime_root(&runtime)?;
    for (key, value) in [
        (b"PROTONPATH".as_slice(), selection.proton.clone()),
        (b"WINEPREFIX", prefix.path.clone()),
        (b"UMU_FOLDERS_PATH", path_bytes(root)?),
    ] {
        environment.insert(key.to_vec(), value);
    }
    for (key, value) in [
        (b"GAMEID".as_slice(), "0"),
        (b"STORE", ""),
        (b"PROTON_VERB", "waitforexitandrun"),
        (b"PROTONFIXES_DISABLE", "1"),
        (b"UMU_RUNTIME_UPDATE", "0"),
        (b"PROTON_LOG", "0"),
        (b"PYTHONDONTWRITEBYTECODE", "1"),
        (b"UMU_HTTP_TIMEOUT", "1"),
        (b"UMU_HTTP_RETRIES", "0"),
    ] {
        environment.insert(key.to_vec(), OsBytes::new(value.as_bytes())?);
    }
    let arguments: Vec<_> = std::iter::once(profile.executable.clone())
        .chain(profile.arguments.iter().cloned())
        .collect();
    let environment: Vec<_> = environment
        .into_iter()
        .map(|(k, v)| Ok((OsBytes::new(k)?, v)))
        .collect::<Result<_>>()?;
    let mut effective = profile.clone();
    effective.executable = selection.executable.clone();
    effective.arguments = arguments.clone();
    effective.environment = environment
        .iter()
        .map(|(key, value)| crate::domain::EnvDelta {
            key: key.clone(),
            value: Some(value.clone()),
        })
        .collect();
    effective.validate()?;
    Ok(PreparedCommand {
        source: profile.clone(),
        executable: selection.executable.clone(),
        arguments,
        cwd: profile.cwd.clone(),
        environment,
        prefix,
        selection: selection.clone(),
        umu_version: versions.umu,
        proton_version: versions.proton,
        runtime_version: versions.pair.runtime_version().into(),
        validated: versions.pair.validated,
    })
}
fn runtime_root(runtime: &Path) -> Result<&Path> {
    if !["steamrt3", "steamrt4"]
        .iter()
        .any(|name| runtime.file_name() == Some(std::ffi::OsStr::new(name)))
        || runtime.parent().and_then(Path::file_name) != Some(std::ffi::OsStr::new("umu"))
    {
        return Err(Error::Invalid("The prepared runtime must use a supported umu/steamrt3 or umu/steamrt4 layout; no directory was moved.".into()));
    }
    runtime
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| Error::Invalid("The prepared UMU root is invalid.".into()))
}
/// D-010: environment keys Rustris or the runner own, which the review shows.
pub(crate) fn runner_owned(key: &[u8]) -> bool {
    reserved(key)
}
fn reserved(key: &[u8]) -> bool {
    RESERVED.contains(&key)
        || [
            b"UMU_".as_slice(),
            b"STEAM_COMPAT_",
            b"PROTON_",
            b"PROTONFIXES_",
            b"PYTHON",
        ]
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || [b"EXE".as_slice(), b"SteamAppId", b"SteamGameId"].contains(&key)
}
fn path_bytes(path: &Path) -> Result<OsBytes> {
    OsBytes::new(path.as_os_str().as_bytes())
}
