#![expect(
    clippy::unwrap_used,
    reason = "Explicit private Windows fixture assertions"
)]
use rustris_core::{
    bytes::OsBytes,
    domain::{EnvDelta, ProfileInput, Runner},
    paths::Paths,
    runners::umu::{self, Selection},
    sessions::{
        BaseEnvironment, SessionState,
        native::{self, LaunchOptions, LogCoverage, Operation},
    },
    store::Store,
};
use std::{
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use uuid::Uuid;
struct Case {
    root: PathBuf,
    paths: Paths,
    profile: ProfileInput,
    selection: Selection,
}
fn bytes(path: &std::path::Path) -> OsBytes {
    OsBytes::new(path.as_os_str().as_bytes()).unwrap()
}
fn windows(path: &std::path::Path) -> OsBytes {
    OsBytes::new(format!("Z:{}", path.to_str().unwrap().replace('/', "\\")).into_bytes()).unwrap()
}
fn runtime(inputs: &std::path::Path) -> OsBytes {
    let name = std::env::var("RUSTRIS_UMU_FIXTURE_RUNTIME").unwrap_or_else(|_| "steamrt3".into());
    assert!(name == "steamrt3" || name == "steamrt4");
    bytes(&inputs.join("data/umu").join(name))
}
impl Case {
    fn create(mode: &str) -> Self {
        let inputs = PathBuf::from(std::env::var_os("RUSTRIS_UMU_FIXTURE_ROOT").unwrap());
        let root = inputs
            .join("cases")
            .join(Uuid::new_v4().simple().to_string());
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        fs::create_dir(root.join("prefix")).unwrap();
        fs::create_dir(root.join("cwd")).unwrap();
        fs::create_dir(root.join("game-xdg")).unwrap();
        let executable = inputs.join("windows-fixture.exe");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o600)).unwrap();
        let profile = ProfileInput {
            name: "Production Windows fixture".into(),
            executable: bytes(&executable),
            cwd: bytes(&root.join("cwd")),
            arguments: vec![
                windows(&root.join("windows.json")),
                OsBytes::new(mode.as_bytes()).unwrap(),
                OsBytes::new(b"8000").unwrap(),
                OsBytes::new([]).unwrap(),
                OsBytes::new("spaces and 🦀".as_bytes()).unwrap(),
                OsBytes::new(b"quotes'\"").unwrap(),
                OsBytes::new(b"$(touch never); echo nope").unwrap(),
            ],
            environment: vec![
                EnvDelta {
                    key: OsBytes::new(b"XDG_DATA_HOME").unwrap(),
                    value: Some(bytes(&root.join("game-xdg"))),
                },
                EnvDelta {
                    key: OsBytes::new(b"WINEDEBUG").unwrap(),
                    value: Some(OsBytes::new(b"-all").unwrap()),
                },
            ],
            runner: Runner::Umu,
            prefix: Some(bytes(&root.join("prefix"))),
        };
        let selection = Selection {
            executable: OsBytes::new(b"/usr/bin/umu-run").unwrap(),
            proton: bytes(&inputs.join("proton")),
            runtime: runtime(&inputs),
        };
        let paths = Paths::in_root(&root.join("library"));
        Self {
            root,
            paths,
            profile,
            selection,
        }
    }
    fn open(root: PathBuf) -> Self {
        let inputs = PathBuf::from(std::env::var_os("RUSTRIS_UMU_FIXTURE_ROOT").unwrap());
        let paths = Paths::in_root(&root.join("library"));
        let store = Store::open_existing(paths.clone()).unwrap();
        let profile = store.snapshot().unwrap().entries[0].profile.clone();
        Self {
            root,
            paths,
            profile,
            selection: Selection {
                executable: OsBytes::new(b"/usr/bin/umu-run").unwrap(),
                proton: bytes(&inputs.join("proton")),
                runtime: runtime(&inputs),
            },
        }
    }
    fn options(&self) -> LaunchOptions {
        LaunchOptions {
            helper: PathBuf::from(env!("CARGO_BIN_EXE_rustris-session")),
            runtime: native::private_runtime(Some(&self.root)).unwrap(),
            state: self.root.join("state"),
        }
    }
    fn prepared(&self) -> umu::PreparedCommand {
        let result = umu::inspect(
            &self.selection,
            &self.profile,
            &BaseEnvironment::capture().unwrap(),
        )
        .unwrap();
        assert!(result.command.is_some(), "{:?}", result.evidence.findings);
        result.command.unwrap()
    }
    fn launch(&self) -> native::LaunchOutcome {
        let plan = self.prepared();
        let mut store = Store::open(self.paths.clone()).unwrap();
        let game = store.save(None, &self.profile).unwrap();
        native::launch_umu(
            self.paths.clone(),
            game,
            1,
            &plan,
            self.options(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }
    fn report(&self) -> Option<serde_json::Value> {
        let path = self.root.join("windows.json");
        if !path.exists() {
            return None;
        }
        Some(serde_json::from_slice(&fs::read(path).unwrap()).unwrap())
    }
    fn finish(&self, id: Uuid) -> rustris_core::sessions::Session {
        let store = Store::open_existing(self.paths.clone()).unwrap();
        let finished = wait(
            || {
                let record = store.session(id).unwrap();
                if matches!(
                    record.state,
                    SessionState::Exited | SessionState::Stopped | SessionState::LaunchFailed
                ) {
                    Some(record)
                } else {
                    None
                }
            },
            Duration::from_secs(40),
        );
        let evidence = serde_json::json!({"session":id,"state":finished.state.key(),"root_exit_code":finished.exit_code,"completion":serde_json::from_str::<serde_json::Value>(&finished.detail).ok(),"reserved":store.session_overview(None).unwrap().iter().any(|s|s.id==id && s.reserved)});
        fs::write(
            self.root.join(format!("observed-{id}.json")),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        finished
    }
}
fn wait<T>(mut predicate: impl FnMut() -> Option<T>, timeout: Duration) -> T {
    let end = Instant::now() + timeout;
    loop {
        if let Some(value) = predicate() {
            return value;
        }
        assert!(Instant::now() < end, "Production UMU fixture deadline");
        std::thread::sleep(Duration::from_millis(25));
    }
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_natural_exit_and_offline_reuse() {
    let case = Case::create("echo");
    let launched = case.launch();
    let report = wait(|| case.report(), Duration::from_secs(40));
    assert_eq!(report["protonfixes_disable"], "1");
    assert_eq!(report["token"], "");
    assert!(!case.root.join("game-xdg/umu").exists());
    assert_eq!(launched.state, SessionState::Running);
    assert_eq!(report["cwd_marker_ok"], 1);
    assert!(case.root.join("cwd/f003-cwd-marker.txt").exists());
    assert!(
        case.root
            .join("prefix/drive_c/f003-fixture-marker.txt")
            .exists()
    );
    let expected = serde_json::json!([
        "8000",
        "",
        "spaces and 🦀",
        "quotes'\"",
        "$(touch never); echo nope"
    ]);
    assert_eq!(
        &report["argv"].as_array().unwrap()[3..],
        expected.as_array().unwrap()
    );
    assert!(!case.root.join("cwd/never").exists());
    let finished = case.finish(launched.session);
    assert_eq!(finished.state, SessionState::Exited, "{}", finished.detail);
    let completion: native::CompletionEvidence = serde_json::from_str(&finished.detail).unwrap();
    assert_eq!(
        completion.log_coverage,
        LogCoverage::RunnerStreamsApplicationUnverified
    );
    assert_eq!(completion.game_health, "unknown");
    fs::rename(
        case.root.join("windows.json"),
        case.root.join("first-windows.json"),
    )
    .unwrap();
    let store = Store::open_existing(case.paths.clone()).unwrap();
    let game = store.snapshot().unwrap().entries[0].game_id;
    let plan = case.prepared();
    let second = native::launch_umu(
        case.paths.clone(),
        game,
        1,
        &plan,
        case.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(case.finish(second.session).state, SessionState::Exited);
    assert!(case.report().is_some());
    assert!(
        store
            .session_overview(None)
            .unwrap()
            .iter()
            .all(|s| !s.reserved)
    );
    fs::write(case.root.join("acceptance.json"),serde_json::to_vec_pretty(&serde_json::json!({"natural":true,"offline_reuse":true,"windows_stdout_ok":report["stdout_ok"],"windows_stderr_ok":report["stderr_ok"],"coverage":completion.log_coverage})).unwrap()).unwrap();
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_detach_reconnect_and_owned_stop_preserve_sentinel() {
    let case = Case::create("hold");
    let launched = case.launch();
    let client = launched.client.unwrap();
    let initial = wait(|| case.report(), Duration::from_secs(40));
    let mut sentinel = std::process::Command::new("/usr/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    assert!(client.request(Operation::Detach).unwrap().accepted);
    let store = Store::open_existing(case.paths.clone()).unwrap();
    assert_eq!(
        store.session(launched.session).unwrap().state,
        SessionState::Unknown
    );
    wait(
        || {
            let report = case.report()?;
            (report["ticks"].as_u64().unwrap() > initial["ticks"].as_u64().unwrap()).then_some(())
        },
        Duration::from_secs(3),
    );
    let client = native::reconnect(case.paths.clone(), launched.session).unwrap();
    assert!(client.request(Operation::Stop).unwrap().accepted);
    assert_eq!(case.finish(launched.session).state, SessionState::Stopped);
    assert!(sentinel.try_wait().unwrap().is_none());
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_force_requires_stop_and_preserves_sentinel() {
    let case = Case::create("hold");
    let launched = case.launch();
    let client = launched.client.unwrap();
    wait(|| case.report(), Duration::from_secs(40));
    let mut sentinel = std::process::Command::new("/usr/bin/sleep")
        .arg("60")
        .spawn()
        .unwrap();
    assert!(!client.request(Operation::Force).unwrap().accepted);
    assert!(client.request(Operation::Stop).unwrap().accepted);
    assert!(client.request(Operation::Force).unwrap().accepted);
    assert_eq!(case.finish(launched.session).state, SessionState::Stopped);
    assert!(sentinel.try_wait().unwrap().is_none());
    sentinel.kill().unwrap();
    sentinel.wait().unwrap();
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_missing_runtime_after_prepare_spawns_nothing() {
    let case = Case::create("echo");
    let plan = case.prepared();
    let marker = case.selection.runtime.path().join(".installed.ok");
    let moved = marker.with_extension("fixture-held");
    fs::rename(&marker, &moved).unwrap();
    let mut store = Store::open(case.paths.clone()).unwrap();
    let game = store.save(None, &case.profile).unwrap();
    let result = native::launch_umu(
        case.paths.clone(),
        game,
        1,
        &plan,
        case.options(),
        &AtomicBool::new(false),
    );
    fs::rename(&moved, &marker).unwrap();
    let outcome = result.unwrap();
    assert_eq!(outcome.state, SessionState::LaunchFailed);
    assert!(outcome.client.is_none());
    assert!(case.report().is_none());
    assert!(store.session(outcome.session).unwrap().helper.is_none());
    assert!(!case.root.join("prefix/drive_c").exists());
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_cancel_before_spawn_creates_no_helper() {
    let case = Case::create("echo");
    let plan = case.prepared();
    let mut store = Store::open(case.paths.clone()).unwrap();
    let game = store.save(None, &case.profile).unwrap();
    let outcome = native::launch_umu(
        case.paths.clone(),
        game,
        1,
        &plan,
        case.options(),
        &AtomicBool::new(true),
    )
    .unwrap();
    assert_eq!(outcome.state, SessionState::Cancelled);
    assert!(outcome.client.is_none());
    assert!(store.session(outcome.session).unwrap().helper.is_none());
    assert!(case.report().is_none());
    assert!(!case.root.join("prefix/drive_c").exists());
    assert!(
        store
            .session_overview(None)
            .unwrap()
            .iter()
            .all(|s| !s.reserved)
    );
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_helper_loss_keeps_unknown_reservation() {
    let case = Case::create("hold");
    let launched = case.launch();
    wait(|| case.report(), Duration::from_secs(40));
    let mut store = Store::open_existing(case.paths.clone()).unwrap();
    let helper = store.session(launched.session).unwrap().helper.unwrap();
    let fd = rustix::process::pidfd_open(
        rustix::process::Pid::from_raw(i32::try_from(helper.pid).unwrap()).unwrap(),
        rustix::process::PidfdFlags::empty(),
    )
    .unwrap();
    rustix::process::pidfd_send_signal(&fd, rustix::process::Signal::KILL).unwrap();
    wait(
        || (store.session(launched.session).unwrap().state == SessionState::Unknown).then_some(()),
        Duration::from_secs(15),
    );
    assert!(native::reconnect(case.paths.clone(), launched.session).is_err());
    let game = store.snapshot().unwrap().entries[0].game_id;
    assert!(matches!(
        store.reserve_umu_session(game, 1, &case.prepared()),
        Err(rustris_core::Error::SessionBusy)
    ));
    assert!(
        store
            .session_overview(None)
            .unwrap()
            .iter()
            .any(|s| s.id == launched.session && s.reserved)
    );
}
#[test]
#[ignore = "Invoked only as the owned controller child by actual_umu_controller_crash"]
fn fixture_umu_controller_child() {
    let root = PathBuf::from(std::env::var_os("RUSTRIS_UMU_CONTROLLER_CASE").unwrap());
    let case = Case::open(root);
    let plan = case.prepared();
    let store = Store::open_existing(case.paths.clone()).unwrap();
    let game = store.snapshot().unwrap().entries[0].game_id;
    let launched = native::launch_umu(
        case.paths.clone(),
        game,
        1,
        &plan,
        case.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    fs::write(
        case.root.join("controller-ready"),
        launched.session.to_string(),
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(60));
}
#[test]
#[ignore = "Requires explicitly prepared isolated runtime and Windows inputs"]
fn actual_umu_controller_crash() {
    let case = Case::create("hold");
    let mut store = Store::open(case.paths.clone()).unwrap();
    store.save(None, &case.profile).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "fixture_umu_controller_child",
            "--nocapture",
        ])
        .env("RUSTRIS_UMU_CONTROLLER_CASE", &case.root)
        .spawn()
        .unwrap();
    let id = wait(
        || fs::read_to_string(case.root.join("controller-ready")).ok(),
        Duration::from_secs(20),
    );
    let id = Uuid::parse_str(&id).unwrap();
    let initial = wait(|| case.report(), Duration::from_secs(40));
    assert!(child.try_wait().unwrap().is_none());
    child.kill().unwrap();
    let status = child.wait().unwrap();
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(status.signal(), Some(9));
    wait(
        || {
            let report = case.report()?;
            (report["ticks"].as_u64().unwrap() > initial["ticks"].as_u64().unwrap()).then_some(())
        },
        Duration::from_secs(3),
    );
    let client = native::reconnect(case.paths.clone(), id).unwrap();
    assert!(client.request(Operation::Stop).unwrap().accepted);
    assert_eq!(case.finish(id).state, SessionState::Stopped);
}
