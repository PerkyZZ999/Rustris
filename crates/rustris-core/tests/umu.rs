#![expect(clippy::unwrap_used, reason = "Isolated fixture assertions")]
use rustris_core::{
    bytes::OsBytes,
    domain::{EnvDelta, ProfileInput, Runner},
    runners::umu::{self, Selection},
    sessions::BaseEnvironment,
};
use std::{
    fs,
    io::Write,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, OpenOptionsExt, symlink},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use uuid::Uuid;
struct Fixture {
    root: PathBuf,
    selection: Selection,
    profile: ProfileInput,
}
fn bytes(path: &Path) -> OsBytes {
    OsBytes::new(path.as_os_str().as_bytes()).unwrap()
}
fn write(path: &Path, data: &[u8], mode: u32) {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(mode)
        .open(path)
        .unwrap();
    file.write_all(data).unwrap();
}
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!("/tmp/ru-{}", Uuid::new_v4().simple()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let raw = root.join(std::ffi::OsStr::from_bytes(b"raw_\xff"));
        fs::create_dir(&raw).unwrap();
        let proton = raw.join("proton");
        let runtime = raw.join("umu/steamrt3");
        fs::create_dir_all(runtime.join("pressure-vessel/bin")).unwrap();
        fs::create_dir_all(runtime.join("sniper_platform_3.0.20260805.254768/files")).unwrap();
        fs::create_dir(&proton).unwrap();
        // Like a real build, the script states the prefix version it writes.
        write(
            &proton.join("proton"),
            b"#!/usr/bin/false\nCURRENT_PREFIX_VERSION=\"UMU-Proton-10.0-4\"\n",
            0o700,
        );
        for path in [
            runtime.join("_v2-entry-point"),
            runtime.join("pressure-vessel/bin/pressure-vessel-wrap"),
            raw.join("umu/umu-shim"),
        ] {
            write(&path, b"#!/usr/bin/false\n", 0o700);
        }
        write(
            &proton.join("version"),
            b"1774856027 UMU-Proton-10.0-4\n",
            0o600,
        );
        write(
            &proton.join("toolmanifest.vdf"),
            b"\"require_tool_appid\" \"1628350\"\n",
            0o600,
        );
        write(
            &runtime.join("VERSIONS.txt"),
            b"sniper\t3.0.20260805.254768\tsniper\n",
            0o600,
        );
        write(&runtime.join(".installed.ok"), b"ok\n", 0o600);
        let tool = raw.join("umu-run");
        write(&tool,b"#!/usr/bin/python3\nimport sys,os,json\nopen('probe.json','w').write(json.dumps({'args':sys.argv[1:],'cwd':os.getcwdb().hex(),'path':os.environ.get('PATH'),'empty':os.environ.get('EMPTY'),'unset':os.environ.get('UNSET'),'credential':os.environ.get('CREDENTIAL')}))\nprint('umu-launcher version 1.4.3 (fixture)')\n",0o700);
        let prefix = raw.join("prefix");
        fs::create_dir(&prefix).unwrap();
        symlink(".", prefix.join("pfx")).unwrap();
        let executable = raw.join("game.exe");
        write(&executable, b"Not executed by preparation", 0o600);
        Self {
            selection: Selection {
                executable: bytes(&tool),
                proton: bytes(&proton),
                runtime: bytes(&runtime),
            },
            profile: ProfileInput {
                name: "UMU fixture".into(),
                executable: bytes(&executable),
                cwd: bytes(&raw),
                arguments: vec![
                    OsBytes::new([]).unwrap(),
                    OsBytes::new(b"literal ;$(never)\xff").unwrap(),
                ],
                environment: vec![
                    EnvDelta {
                        key: OsBytes::new(b"EMPTY").unwrap(),
                        value: Some(OsBytes::new([]).unwrap()),
                    },
                    EnvDelta {
                        key: OsBytes::new(b"UNSET").unwrap(),
                        value: None,
                    },
                ],
                runner: Runner::Umu,
                prefix: Some(bytes(&prefix)),
            },
            root,
        }
    }
    fn report(&self) -> PathBuf {
        self.profile.cwd.path().join("probe.json")
    }
    fn ge() -> Self {
        let mut fixture = Self::new();
        let old = fixture.selection.runtime.path();
        let runtime = old.with_file_name("steamrt4");
        fs::rename(&old, &runtime).unwrap();
        fs::remove_dir_all(runtime.join("sniper_platform_3.0.20260805.254768")).unwrap();
        fs::create_dir_all(runtime.join("steamrt4_platform_4.0.20261021.265997/files")).unwrap();
        fs::write(
            runtime.join("VERSIONS.txt"),
            b"steamrt4\t4.0.20261021.265997\tsteamrt4\n",
        )
        .unwrap();
        fs::write(
            fixture.selection.proton.path().join("version"),
            b"1786437966 GE-Proton11-5\n",
        )
        .unwrap();
        fs::write(
            fixture.selection.proton.path().join("proton"),
            b"#!/usr/bin/false\nCURRENT_PREFIX_VERSION=\"GE-Proton11-5\"\n",
        )
        .unwrap();
        fs::write(
            fixture.selection.proton.path().join("toolmanifest.vdf"),
            b"\"require_tool_appid\" \"4183110\"\n",
        )
        .unwrap();
        fs::write(
            fixture
                .profile
                .prefix
                .as_ref()
                .unwrap()
                .path()
                .join("version"),
            b"GE-Proton11-5\n",
        )
        .unwrap();
        fixture.selection.runtime = bytes(&runtime);
        fixture
    }
    fn inspect(&self) -> umu::Preparation {
        umu::inspect(
            &self.selection,
            &self.profile,
            &BaseEnvironment::from_pairs([
                (b"PATH".to_vec(), b"/usr/bin".to_vec()),
                (b"UNSET".to_vec(), b"remove".to_vec()),
                (b"CREDENTIAL".to_vec(), b"never-inherit".to_vec()),
            ])
            .unwrap(),
        )
        .unwrap()
    }
}
#[test]
fn ge_pair_preserves_exact_values_and_frozen_provenance() {
    use rustris_core::{paths::Paths, store::Store};
    let fixture = Fixture::ge();
    let plan = fixture.inspect().command.unwrap();
    assert_eq!(plan.proton_version(), "1786437966 GE-Proton11-5");
    assert_eq!(plan.runtime_version(), "4.0.20261021.265997");
    assert_eq!(&plan.arguments()[1..], fixture.profile.arguments);
    let paths = Paths::in_root(&fixture.root.join("ge-library"));
    let mut store = Store::open(paths.clone()).unwrap();
    let game = store.save(None, &fixture.profile).unwrap();
    let (session, _) = store.reserve_umu_session(game, 1, &plan).unwrap();
    drop(store);
    let reopened = Store::open_existing(paths).unwrap();
    let runtime = reopened
        .session(session.id)
        .unwrap()
        .snapshot
        .runtime
        .unwrap();
    assert_eq!(runtime.proton_version, "1786437966 GE-Proton11-5");
    assert_eq!(runtime.runtime_version, "4.0.20261021.265997");
    assert_eq!(runtime.selection.runtime, fixture.selection.runtime);
}
#[test]
fn ge_pair_rejects_cross_versions_wrong_manifest_and_prefix_before_probe() {
    let fixture = Fixture::ge();
    let proton = fixture.selection.proton.path();
    let runtime = fixture.selection.runtime.path();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    // D-012: another build of the same Proton is allowed but not validated.
    assert!(fixture.inspect().command.unwrap().validated());
    let _ = fs::remove_file(fixture.report());
    fs::write(proton.join("version"), b"1786437967 GE-Proton11-5\n").unwrap();
    assert!(!fixture.inspect().command.unwrap().validated());
    let _ = fs::remove_file(fixture.report());
    fs::write(proton.join("version"), b"1786437966 GE-Proton11-5\n").unwrap();
    // A Proton built for another runtime, a runtime without its platform files,
    // and a prefix made by another Proton are still refused before the probe.
    for (path, wrong, correct) in [
        (
            proton.join("toolmanifest.vdf"),
            b"\"require_tool_appid\" \"1628350\"\n".as_slice(),
            b"\"require_tool_appid\" \"4183110\"\n".as_slice(),
        ),
        (
            runtime.join("VERSIONS.txt"),
            b"steamrt4\t4.0.20261021.265998\n".as_slice(),
            b"steamrt4\t4.0.20261021.265997\n".as_slice(),
        ),
        (
            prefix.join("version"),
            b"UMU-Proton-10.0-4\n".as_slice(),
            b"GE-Proton11-5\n".as_slice(),
        ),
    ] {
        fs::write(&path, wrong).unwrap();
        assert!(fixture.inspect().command.is_none());
        assert!(!fixture.report().exists());
        fs::write(path, correct).unwrap();
    }
    fs::remove_dir_all(runtime.join("steamrt4_platform_4.0.20261021.265997")).unwrap();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(
        result
            .evidence
            .findings
            .iter()
            .any(|finding| finding.rule == "umu.runtime_platform.preparation")
    );
    assert!(!fixture.report().exists());
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn preparation_preserves_exact_bytes_and_runner_owned_controls_without_launching_game() {
    let fixture = Fixture::new();
    let prepared = fixture.inspect();
    assert!(
        prepared.evidence.findings.is_empty(),
        "{:?}",
        prepared.evidence.findings
    );
    let command = prepared.command.unwrap();
    assert_eq!(command.arguments()[0], fixture.profile.executable);
    assert_eq!(&command.arguments()[1..], fixture.profile.arguments);
    assert_eq!(*command.cwd(), fixture.profile.cwd);
    assert_eq!(
        command.prefix().path,
        fixture.profile.prefix.clone().unwrap()
    );
    let env: std::collections::BTreeMap<_, _> = command
        .environment()
        .iter()
        .cloned()
        .map(|(k, v)| (k.as_bytes().to_vec(), v))
        .collect();
    assert_eq!(env[b"GAMEID".as_slice()].as_bytes(), b"0");
    assert_eq!(
        env[b"PROTON_VERB".as_slice()].as_bytes(),
        b"waitforexitandrun"
    );
    assert_eq!(env[b"UMU_RUNTIME_UPDATE".as_slice()].as_bytes(), b"0");
    assert_eq!(env[b"PROTONFIXES_DISABLE".as_slice()].as_bytes(), b"1");
    assert_eq!(env[b"PROTON_LOG".as_slice()].as_bytes(), b"0");
    assert_eq!(
        env[b"UMU_FOLDERS_PATH".as_slice()].path(),
        fixture.profile.cwd.path()
    );
    assert!(!env.contains_key(b"CREDENTIAL".as_slice()));
    assert!(!env.contains_key(b"UNSET".as_slice()));
    assert_eq!(env[b"EMPTY".as_slice()].as_bytes(), b"");
    let query: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.report()).unwrap()).unwrap();
    assert_eq!(query["args"], serde_json::json!(["--version"]));
    assert_eq!(query["credential"], serde_json::Value::Null);
    assert_eq!(query["empty"], "");
    assert_eq!(
        fs::read(fixture.profile.executable.path()).unwrap(),
        b"Not executed by preparation"
    );
    assert!(!fixture.root.join("never").exists());
}
#[test]
fn missing_preparation_never_invokes_even_the_version_query() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.selection.runtime.path().join(".installed.ok")).unwrap();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(!fixture.report().exists());
    assert!(
        result
            .evidence
            .findings
            .iter()
            .any(|f| f.rule == "umu.runtime_marker.preparation")
    );
    for f in &result.evidence.findings {
        assert!(
            result
                .evidence
                .observations
                .iter()
                .any(|o| o.id == f.evidence && o.observed_ms > 0 && !o.source.is_empty())
        );
    }
}
#[test]
fn populated_prefix_version_mismatch_or_unknown_blocks_before_probe() {
    let fixture = Fixture::new();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    let marker = prefix.join("version");
    fs::write(&marker, b"GE-Proton11-5\n").unwrap();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(
        result
            .evidence
            .findings
            .iter()
            .any(|f| f.rule == "umu.prefix_version.preparation")
    );
    assert!(!fixture.report().exists());
    assert_eq!(fs::read(&marker).unwrap(), b"GE-Proton11-5\n");
    fs::remove_file(&marker).unwrap();
    fs::write(prefix.join("user.reg"), b"existing").unwrap();
    assert!(fixture.inspect().command.is_none());
    assert!(!fixture.report().exists());
    fs::write(&marker, b"UMU-Proton-10.0-4\n").unwrap();
    assert!(fixture.inspect().command.is_some());
}
#[test]
fn prefix_version_marker_symlink_and_oversize_are_rejected() {
    let fixture = Fixture::new();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    let marker = prefix.join("version");
    symlink(fixture.selection.proton.path().join("version"), &marker).unwrap();
    assert!(fixture.inspect().command.is_none());
    assert!(!fixture.report().exists());
    fs::remove_file(&marker).unwrap();
    fs::write(&marker, vec![b'x'; 129]).unwrap();
    assert!(fixture.inspect().command.is_none());
    assert!(!fixture.report().exists());
}
#[test]
fn nested_and_broken_prefixes_fail_without_repair_or_probe() {
    let fixture = Fixture::new();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    fs::remove_file(prefix.join("pfx")).unwrap();
    fs::create_dir(prefix.join("nested")).unwrap();
    symlink("nested", prefix.join("pfx")).unwrap();
    assert!(fixture.inspect().command.is_none());
    assert_eq!(
        fs::read_link(prefix.join("pfx")).unwrap(),
        PathBuf::from("nested")
    );
    fs::remove_file(prefix.join("pfx")).unwrap();
    symlink("absent", prefix.join("pfx")).unwrap();
    assert!(fixture.inspect().command.is_none());
    assert!(
        fs::symlink_metadata(prefix.join("pfx"))
            .unwrap()
            .is_symlink()
    );
    assert!(!fixture.report().exists());
}
#[test]
fn reserved_set_and_unset_cannot_override_policy() {
    let mut fixture = Fixture::new();
    for key in [
        "PROTONPATH",
        "WINEPREFIX",
        "GAMEID",
        "STORE",
        "PROTON_VERB",
        "PROTONFIXES_DISABLE",
        "UMU_RUNTIME_UPDATE",
        "UMU_NO_PROTON",
        "UMU_NO_RUNTIME",
        "RUNTIMEPATH",
        "UMU_FOLDERS_PATH",
        "LD_PRELOAD",
        "LD_AUDIT",
        "LD_LIBRARY_PATH",
        "PYTHONPATH",
        "PYTHONHOME",
        "PROTON_LOG",
        "PROTON_LOG_DIR",
        "UMU_CONTAINER_NSENTER",
        "STEAM_COMPAT_DATA_PATH",
        "PROTON_DUMP_DEBUG_COMMANDS",
        "PYTHONINSPECT",
        "EXE",
        "SteamAppId",
        "SteamGameId",
    ] {
        for value in [None, Some(OsBytes::new(b"value").unwrap())] {
            fixture.profile.environment = vec![EnvDelta {
                key: OsBytes::new(key.as_bytes()).unwrap(),
                value,
            }];
            assert!(
                umu::inspect(
                    &fixture.selection,
                    &fixture.profile,
                    &BaseEnvironment::default()
                )
                .is_err(),
                "{key}"
            );
        }
    }
    assert!(!fixture.report().exists());
}
#[test]
fn custom_settings_even_broken_symlink_are_refused_without_execution() {
    let fixture = Fixture::new();
    let settings = fixture.selection.proton.path().join("user_settings.py");
    symlink("absent", &settings).unwrap();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(
        result
            .evidence
            .findings
            .iter()
            .any(|f| f.rule == "umu.custom_settings.preparation")
    );
    assert!(!fixture.report().exists());
    assert!(fs::symlink_metadata(settings).unwrap().is_symlink());
}
#[test]
fn unvalidated_pair_is_labelled_and_oversized_manifest_fails_closed() {
    let fixture = Fixture::new();
    let version = fixture.selection.proton.path().join("version");
    // D-012: an untested Proton release prepares, labelled as not validated.
    fs::write(&version, b"1774856027 UMU-Proton-11\n").unwrap();
    let prepared = fixture.inspect().command.unwrap();
    assert!(!prepared.validated());
    assert_eq!(prepared.proton_version(), "1774856027 UMU-Proton-11");
    let _ = fs::remove_file(fixture.report());
    fs::write(version, b"1774856027 UMU-Proton-10.0-4\n").unwrap();
    fs::write(
        fixture.selection.runtime.path().join("VERSIONS.txt"),
        vec![b'x'; 65537],
    )
    .unwrap();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(
        result
            .evidence
            .findings
            .iter()
            .any(|f| f.message.contains("64 KiB"))
    );
    assert!(!fixture.report().exists());
}
#[test]
fn unvalidated_tool_and_output_flood_are_bounded() {
    let fixture = Fixture::new();
    let tool = fixture.selection.executable.path();
    // D-012: another UMU release is accepted but not validated.
    fs::write(
        &tool,
        "#!/usr/bin/python3\nprint('umu-launcher version 1.4.30 (fixture)')\n",
    )
    .unwrap();
    assert!(!fixture.inspect().command.unwrap().validated());
    for body in [
        "print('not umu at all')",
        "import os;os.write(1,b'x'*131072)",
        "import os;os.write(2,b'x'*131072)",
        "import os;os.write(1,b'\\xff')",
    ] {
        fs::write(&tool, format!("#!/usr/bin/python3\n{body}\n")).unwrap();
        let started = Instant::now();
        let result = fixture.inspect();
        assert!(result.command.is_none());
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}
#[test]
fn stalled_version_query_is_reaped_without_touching_a_sentinel() {
    let fixture = Fixture::new();
    fs::write(
        fixture.selection.executable.path(),
        b"#!/usr/bin/python3\nimport time\ntime.sleep(10)\n",
    )
    .unwrap();
    let mut sentinel = std::process::Command::new("/usr/bin/sleep")
        .arg("10")
        .spawn()
        .unwrap();
    let started = Instant::now();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(sentinel.try_wait().unwrap().is_none());
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}
#[test]
fn inaccessible_executable_is_not_missing_and_is_not_chmodded() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let fixture = Fixture::new();
    let tool = fixture.selection.executable.path();
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o600)).unwrap();
    let result = fixture.inspect();
    assert!(result.command.is_none());
    assert!(
        result
            .evidence
            .observations
            .iter()
            .any(|o| o.resource == "umu" && o.outcome == "inaccessible" && o.errno == Some(13))
    );
    assert_eq!(fs::metadata(tool).unwrap().mode() & 0o777, 0o600);
}
#[test]
#[ignore = "Explicit read-only inspection of user-identified host inputs; never launches a game"]
fn identified_host_inputs_are_prepared() {
    // The owner's own runner and game paths come from the environment so they never
    // live in the repository: RUSTRIS_REAL_PROTON, RUSTRIS_REAL_RUNTIME,
    // RUSTRIS_REAL_GAME (a Windows .exe) and RUSTRIS_REAL_PREFIX.
    let var = |key: &str| {
        std::env::var_os(key).map(|value| {
            use std::os::unix::ffi::OsStrExt;
            OsBytes::new(value.as_bytes()).unwrap()
        })
    };
    let (Some(proton), Some(runtime), Some(game), Some(prefix)) = (
        var("RUSTRIS_REAL_PROTON"),
        var("RUSTRIS_REAL_RUNTIME"),
        var("RUSTRIS_REAL_GAME"),
        var("RUSTRIS_REAL_PREFIX"),
    ) else {
        eprintln!("Set RUSTRIS_REAL_PROTON, _RUNTIME, _GAME and _PREFIX to run this check.");
        return;
    };
    let cwd = OsBytes::new(std::os::unix::ffi::OsStrExt::as_bytes(
        game.path().parent().unwrap().as_os_str(),
    ))
    .unwrap();
    let selection = Selection {
        executable: OsBytes::new(b"/usr/bin/umu-run").unwrap(),
        proton,
        runtime,
    };
    let profile = ProfileInput {
        name: "Authorized preparation only".into(),
        executable: game,
        cwd,
        arguments: vec![],
        environment: vec![],
        runner: Runner::Umu,
        prefix: Some(prefix),
    };
    let result = umu::inspect(&selection, &profile, &BaseEnvironment::capture().unwrap()).unwrap();
    assert!(result.command.is_some(), "{:?}", result.evidence.findings);
    let command = result.command.unwrap();
    println!(
        "{}",
        serde_json::json!({"evidence":result.evidence,"policy":umu::POLICY,"tool":selection.executable.label(),"proton":selection.proton.label(),"runtime":selection.runtime.label(),"umu_version":command.umu_version(),"proton_version":command.proton_version(),"runtime_version":command.runtime_version(),"prefix":command.prefix().path.label(),"prefix_device":command.prefix().device,"prefix_inode":command.prefix().inode})
    );
}

#[test]
fn effective_command_overhead_cannot_exceed_profile_budgets() {
    let mut fixture = Fixture::new();
    fixture.profile.environment = (0..247)
        .map(|i| EnvDelta {
            key: OsBytes::new(format!("CUSTOM_{i}").into_bytes()).unwrap(),
            value: Some(OsBytes::new(b"value").unwrap()),
        })
        .collect();
    let base = BaseEnvironment::from_pairs([(b"PATH".to_vec(), b"/usr/bin".to_vec())]).unwrap();
    fixture.profile.validate().unwrap();
    assert!(umu::inspect(&fixture.selection, &fixture.profile, &base).is_err());
}
#[test]
fn missing_shim_and_unselected_runtime_lookup_never_bootstrap() {
    let mut fixture = Fixture::new();
    let shim = fixture
        .selection
        .runtime
        .path()
        .parent()
        .unwrap()
        .join("umu-shim");
    fs::remove_file(&shim).unwrap();
    let report = fixture.inspect();
    assert!(report.command.is_none());
    assert!(!fixture.report().exists());
    assert!(!shim.exists());
    write(&shim, b"#!/usr/bin/false\n", 0o700);
    let runtime = fixture.selection.runtime.path();
    let moved = runtime.with_file_name("other-runtime");
    fs::rename(runtime, &moved).unwrap();
    fixture.selection.runtime = bytes(&moved);
    let report = fixture.inspect();
    assert!(report.command.is_none());
    assert!(
        report
            .evidence
            .findings
            .iter()
            .any(|f| f.rule == "umu.runtime_lookup.preparation")
    );
    assert!(!fixture.report().exists());
}

#[test]
fn runner_snapshot_freezes_exact_selection_policy_and_environment() {
    use rustris_core::{paths::Paths, sessions::SessionState, store::Store};
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("library"));
    let mut store = Store::open(paths.clone()).unwrap();
    let game = store.save(None, &fixture.profile).unwrap();
    let plan = fixture.inspect().command.unwrap();
    let (session, _owner) = store.reserve_umu_session(game, 1, &plan).unwrap();
    let runtime = session.snapshot.runtime.as_ref().unwrap();
    assert_eq!(runtime.selection.executable, *plan.executable());
    assert_eq!(runtime.prefix.path, plan.prefix().path);
    assert_eq!(runtime.policy, umu::POLICY);
    assert_eq!(session.snapshot.environment, plan.environment());
    assert!(store.reserve_umu_session(game, 1, &plan).is_err());
    let connection = rusqlite::Connection::open(paths.database()).unwrap();
    assert!(
        connection
            .execute("UPDATE session_runners SET umu_version='changed'", [])
            .is_err()
    );
    assert!(
        connection
            .execute("DELETE FROM session_runners", [])
            .is_err()
    );
    let unknown = store
        .mark_session_unknown(
            session.id,
            session.generation,
            "Synthetic storage fixture; no helper was started.",
        )
        .unwrap();
    store
        .release_unknown_session(session.id, unknown.generation, true)
        .unwrap();
    let mut edited = fixture.profile.clone();
    edited.name = "Changed source".into();
    edited.arguments.push(OsBytes::new(b"new token").unwrap());
    store.save(Some((game, 1)), &edited).unwrap();
    drop(store);
    fs::remove_file(fixture.selection.proton.path().join("version")).unwrap();
    let store = Store::open_existing(paths).unwrap();
    let historical = store.session(session.id).unwrap();
    assert_eq!(historical.state, SessionState::ReleasedUnknown);
    assert_eq!(historical.snapshot.source.profile.name, "UMU fixture");
    assert_eq!(
        historical.snapshot.source.profile.arguments,
        fixture.profile.arguments
    );
    assert_eq!(
        historical.snapshot.runtime.unwrap().selection.proton,
        fixture.selection.proton
    );
}

#[test]
fn prepared_profiles_with_prefix_aliases_remain_reserved_until_confirmed_release() {
    use rustris_core::{Error, paths::Paths, sessions::SessionState, store::Store};
    for fixture in [Fixture::new(), Fixture::ge()] {
        let paths = Paths::in_root(&fixture.root.join("library"));
        let mut store = Store::open(paths).unwrap();
        let first = store.save(None, &fixture.profile).unwrap();
        let first_plan = fixture.inspect().command.unwrap();
        let alias = fixture.root.join("prefix-alias");
        symlink(fixture.profile.prefix.as_ref().unwrap().path(), &alias).unwrap();
        let mut other = fixture.profile.clone();
        other.name = "Aliased prefix fixture".into();
        other.prefix = Some(bytes(&alias));
        let second = store.save(None, &other).unwrap();
        let second_plan = umu::inspect(&fixture.selection, &other, &BaseEnvironment::default())
            .unwrap()
            .command
            .unwrap();
        let (session, _) = store.reserve_umu_session(first, 1, &first_plan).unwrap();
        assert!(matches!(
            store.reserve_umu_session(second, 1, &second_plan),
            Err(Error::SessionBusy)
        ));
        let unknown = store
            .mark_session_unknown(session.id, session.generation, "Synthetic helper loss")
            .unwrap();
        assert!(matches!(
            store.reserve_umu_session(second, 1, &second_plan),
            Err(Error::SessionBusy)
        ));
        assert!(
            store
                .release_unknown_session(session.id, unknown.generation, false)
                .is_err()
        );
        let released = store
            .release_unknown_session(session.id, unknown.generation, true)
            .unwrap();
        assert_eq!(released.state, SessionState::ReleasedUnknown);
        let (new_session, _) = store.reserve_umu_session(second, 1, &second_plan).unwrap();
        assert_eq!(
            new_session.snapshot.runtime.unwrap().prefix.path,
            first_plan.prefix().path
        );
        assert!(
            fs::symlink_metadata(alias)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
#[test]
fn changed_source_or_prefix_cannot_consume_a_prepared_command() {
    use rustris_core::{Error, paths::Paths, store::Store};
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("library"));
    let mut store = Store::open(paths.clone()).unwrap();
    let game = store.save(None, &fixture.profile).unwrap();
    let plan = fixture.inspect().command.unwrap();
    let mut changed = fixture.profile.clone();
    changed.arguments.push(OsBytes::new(b"changed").unwrap());
    store.save(Some((game, 1)), &changed).unwrap();
    assert!(matches!(
        store.reserve_umu_session(game, 2, &plan),
        Err(Error::Conflict)
    ));
    store.save(Some((game, 2)), &fixture.profile).unwrap();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    fs::rename(&prefix, prefix.with_file_name("old-prefix")).unwrap();
    fs::create_dir(&prefix).unwrap();
    symlink(".", prefix.join("pfx")).unwrap();
    assert!(store.reserve_umu_session(game, 3, &plan).is_err());
    let connection = rusqlite::Connection::open(paths.database()).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM launch_sessions", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn private_runner_selection_round_trips_bytes_without_installation_writes() {
    use rustris_core::paths::Paths;
    use std::os::unix::fs::MetadataExt;
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("selection"));
    assert!(paths.read_runner_selection().unwrap().is_none());
    assert!(!paths.config.exists());
    paths.write_runner_selection(&fixture.selection).unwrap();
    let saved = paths.read_runner_selection().unwrap().unwrap();
    assert_eq!(saved.executable, fixture.selection.executable);
    assert_eq!(saved.proton, fixture.selection.proton);
    assert_eq!(saved.runtime, fixture.selection.runtime);
    assert_eq!(
        fs::metadata(paths.config.join("runners.toml"))
            .unwrap()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(fs::metadata(&paths.config).unwrap().mode() & 0o777, 0o700);
    assert!(!paths.data.exists());
    assert_eq!(
        fs::read(fixture.profile.executable.path()).unwrap(),
        b"Not executed by preparation"
    );
    assert!(!fixture.report().exists());
}
#[test]
fn invalid_or_future_runner_selection_is_preserved_and_native_library_stays_usable() {
    use rustris_core::{paths::Paths, store::Store};
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("selection"));
    paths.write_runner_selection(&fixture.selection).unwrap();
    let file = paths.config.join("runners.toml");
    for content in [
        b"version = 999\numu='/'\nproton='/'\nruntime='/'\n".as_slice(),
        b"broken [ TOML",
        b"version=1\numu='relative'\nproton='/'\nruntime='/'\n",
    ] {
        fs::write(&file, content).unwrap();
        assert!(paths.read_runner_selection().is_err());
        assert!(paths.write_runner_selection(&fixture.selection).is_err());
        assert_eq!(fs::read(&file).unwrap(), content);
        assert!(Store::open(paths.clone()).unwrap().snapshot().is_ok());
    }
}
#[test]
fn runner_selection_symlinks_and_fifos_do_not_block_or_overwrite() {
    use rustris_core::paths::Paths;
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("selection"));
    paths.write_runner_selection(&fixture.selection).unwrap();
    let file = paths.config.join("runners.toml");
    fs::remove_file(&file).unwrap();
    symlink("missing", &file).unwrap();
    assert!(paths.read_runner_selection().is_err());
    assert!(paths.write_runner_selection(&fixture.selection).is_err());
    assert!(fs::symlink_metadata(&file).unwrap().is_symlink());
    fs::remove_file(&file).unwrap();
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &file,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let probe = paths.clone();
    let join = std::thread::spawn(move || {
        sender.send(probe.read_runner_selection().is_err()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_secs(1)).unwrap());
    join.join().unwrap();
    assert!(paths.write_runner_selection(&fixture.selection).is_err());
}

#[test]
fn raw_reservation_also_refuses_a_broken_prefix_alias() {
    use rustris_core::{paths::Paths, store::Store};
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("library"));
    let mut store = Store::open(paths.clone()).unwrap();
    let game = store.save(None, &fixture.profile).unwrap();
    let pfx = fixture.profile.prefix.as_ref().unwrap().path().join("pfx");
    fs::remove_file(&pfx).unwrap();
    symlink("missing", &pfx).unwrap();
    assert!(
        store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .is_err()
    );
    assert_eq!(fs::read_link(pfx).unwrap(), PathBuf::from("missing"));
}

#[test]
fn corrupted_runner_reservation_is_refused_before_writable_open() {
    use rustris_core::{paths::Paths, store::Store};
    let fixture = Fixture::new();
    let paths = Paths::in_root(&fixture.root.join("library"));
    let mut store = Store::open(paths.clone()).unwrap();
    let game = store.save(None, &fixture.profile).unwrap();
    let plan = fixture.inspect().command.unwrap();
    store.reserve_umu_session(game, 1, &plan).unwrap();
    drop(store);
    let connection = rusqlite::Connection::open(paths.database()).unwrap();
    connection
        .execute("DROP TRIGGER frozen_runner_update", [])
        .unwrap();
    connection
        .execute(
            "UPDATE session_runners SET prefix_path=?1",
            [b"/different".as_slice()],
        )
        .unwrap();
    drop(connection);
    let before = fs::read(paths.database()).unwrap();
    assert!(
        Store::open_existing(paths.clone())
            .err()
            .unwrap()
            .to_string()
            .contains("runner prefix")
    );
    assert_eq!(fs::read(paths.database()).unwrap(), before);
}

#[test]
fn prefix_guard_uses_the_version_proton_writes_not_the_build_name() {
    // GE-Proton11-12 and Steam's Proton write a prefix version unlike their name.
    let fixture = Fixture::ge();
    let proton = fixture.selection.proton.path();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    fs::write(proton.join("version"), b"1793687165 GE-Proton11-12\n").unwrap();
    fs::write(
        proton.join("proton"),
        b"#!/usr/bin/false\nCURRENT_PREFIX_VERSION=\"11.0-100\"\n",
    )
    .unwrap();
    // A prefix this build already set up is accepted on the next launch.
    fs::write(prefix.join("version"), b"11.0-100\n").unwrap();
    let prepared = fixture.inspect().command.unwrap();
    assert!(!prepared.validated());
    let _ = fs::remove_file(fixture.report());
    // A prefix made by another Proton is still refused.
    fs::write(prefix.join("version"), b"GE-Proton11-5\n").unwrap();
    assert!(fixture.inspect().command.is_none());
    // A build whose script does not state its prefix version fails closed.
    fs::write(prefix.join("version"), b"11.0-100\n").unwrap();
    fs::write(proton.join("proton"), b"#!/usr/bin/false\n").unwrap();
    assert!(fixture.inspect().command.is_none());
    assert!(!fixture.report().exists());
}

#[test]
fn interrupted_first_launch_skeleton_is_not_a_foreign_prefix() {
    let fixture = Fixture::new();
    let prefix = fixture.profile.prefix.as_ref().unwrap().path();
    // What UMU creates before Proton writes its marker.
    for dir in ["shadercache", "gstreamer-1.0", "drive_c/users/steamuser"] {
        fs::create_dir_all(prefix.join(dir)).unwrap();
    }
    fs::write(prefix.join("tracked_files"), b"").unwrap();
    assert!(fixture.inspect().command.is_some());
    let _ = fs::remove_file(fixture.report());
    // Anything Proton itself copies in means the prefix is no longer a skeleton.
    fs::create_dir_all(prefix.join("drive_c/windows")).unwrap();
    let refused = fixture.inspect();
    assert!(refused.command.is_none());
    assert!(
        refused
            .evidence
            .findings
            .iter()
            .any(|f| f.message.contains("first launch may have been interrupted"))
    );
}
