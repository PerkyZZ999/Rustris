#![expect(clippy::unwrap_used, reason = "Explicit isolated fixture assertions")]
use rustris_core::{
    bytes::OsBytes,
    domain::{EnvDelta, ProfileInput, Runner},
    paths::Paths,
    sessions::{
        BaseEnvironment, SessionState,
        native::{self, LaunchOptions},
    },
    store::Store,
};
use std::{
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt},
    },
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
use uuid::Uuid;
struct Fixture {
    root: PathBuf,
    paths: Paths,
}
impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(format!("/tmp/rs-{}", Uuid::new_v4().simple()));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        let paths = Paths::in_root(&root.join("library"));
        Self { root, paths }
    }
    fn input(&self) -> ProfileInput {
        let script = self.root.join("fixture.py");
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&script)
            .unwrap();
        file.write_all(include_bytes!("fixtures/native.py"))
            .unwrap();
        ProfileInput {
            name: "Native fixture".into(),
            executable: OsBytes::new(b"/usr/bin/python3").unwrap(),
            cwd: OsBytes::new(self.root.as_os_str().as_bytes()).unwrap(),
            arguments: vec![
                OsBytes::new(script.as_os_str().as_bytes()).unwrap(),
                OsBytes::new([]).unwrap(),
                OsBytes::new(b"raw_\xff ;$(never)").unwrap(),
            ],
            environment: vec![
                EnvDelta {
                    key: OsBytes::new(b"FIXTURE_REPORT").unwrap(),
                    value: Some(
                        OsBytes::new(self.root.join("report.json").as_os_str().as_bytes()).unwrap(),
                    ),
                },
                EnvDelta {
                    key: OsBytes::new(b"KEY_\xff").unwrap(),
                    value: Some(OsBytes::new(b"value_\xfe").unwrap()),
                },
            ],
            runner: Runner::Native,
            prefix: None,
        }
    }
    fn options(&self) -> LaunchOptions {
        LaunchOptions {
            helper: PathBuf::from(env!("CARGO_BIN_EXE_rustris-session")),
            runtime: native::private_runtime(Some(&self.root)).unwrap(),
            state: self.root.join("state"),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn native_echo_preserves_exact_values_and_observed_exit() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let input = fixture.input();
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    let record = loop {
        let record = store.session(launched.session).unwrap();
        if record.state == SessionState::Exited {
            break record;
        }
        assert!(
            Instant::now() < end,
            "state {:?}: {}",
            record.state,
            record.detail
        );
        std::thread::sleep(Duration::from_millis(25));
    };
    assert_eq!(record.exit_code, Some(7));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("report.json")).unwrap()).unwrap();
    assert_eq!(
        report["argv"],
        serde_json::json!(["", "7261775fff203b24286e6576657229"])
    );
    assert_eq!(report["raw"], "76616c75655ffe");
    assert!(!fixture.root.join("never").exists());
    assert!(
        fs::metadata(
            fixture
                .root
                .join("state/logs")
                .join(format!("{}.log", record.id))
        )
        .unwrap()
        .len()
            > 0
    );
}

fn set(input: &mut ProfileInput, key: &[u8], value: &[u8]) {
    input.environment.push(EnvDelta {
        key: OsBytes::new(key).unwrap(),
        value: Some(OsBytes::new(value).unwrap()),
    });
}
fn wait_state(
    store: &Store,
    session: Uuid,
    expected: SessionState,
) -> rustris_core::sessions::Session {
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let record = store.session(session).unwrap();
        if record.state == expected {
            return record;
        }
        assert!(
            Instant::now() < end,
            "Expected {expected:?}; got {:?}: {}",
            record.state,
            record.detail
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn later_profile_launch_uses_new_values_and_preserves_prior_snapshot() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let original = fixture.input();
    let game = store.save(None, &original).unwrap();
    let first = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let first_record = wait_state(&store, first.session, SessionState::Exited);
    assert_eq!(first_record.exit_code, Some(7));
    assert_eq!(first_record.snapshot.source.profile, original);
    let first_report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("report.json")).unwrap()).unwrap();
    assert_eq!(
        first_report["argv"],
        serde_json::json!(["", "7261775fff203b24286e6576657229"])
    );

    let mut edited = original.clone();
    edited.name = "Edited fixture".into();
    edited.arguments[2] = OsBytes::new(b"new_\xfe ;$(never)").unwrap();
    edited.environment[0].value = Some(
        OsBytes::new(
            fixture
                .root
                .join("edited-report.json")
                .as_os_str()
                .as_bytes(),
        )
        .unwrap(),
    );
    store.save(Some((game, 1)), &edited).unwrap();
    let second = native::launch(
        fixture.paths.clone(),
        game,
        2,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let second_record = wait_state(&store, second.session, SessionState::Exited);
    assert_eq!(second_record.exit_code, Some(7));
    assert_eq!(second_record.snapshot.source.revision, 2);
    assert_eq!(second_record.snapshot.source.profile, edited);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("edited-report.json")).unwrap())
            .unwrap();
    assert_eq!(
        report["argv"],
        serde_json::json!(["", "6e65775ffe203b24286e6576657229"])
    );
    assert!(!fixture.root.join("never").exists());

    drop(store);
    let reopened = Store::open(fixture.paths.clone()).unwrap();
    let historical = reopened.session(first.session).unwrap();
    assert_eq!(historical.snapshot.source.revision, 1);
    assert_eq!(historical.snapshot.source.profile, original);
    assert_eq!(
        reopened
            .session(second.session)
            .unwrap()
            .snapshot
            .source
            .profile,
        edited
    );
}

#[test]
fn fast_exit_releases_only_after_actual_observation() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    input.executable = OsBytes::new(b"/usr/bin/true").unwrap();
    input.arguments.clear();
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let record = wait_state(&store, launched.session, SessionState::Exited);
    assert_eq!(record.exit_code, Some(0));
    assert!(
        store
            .reserve_session(game, 1, &BaseEnvironment::default())
            .is_ok()
    );
}
#[test]
fn accepted_cancel_before_helper_start_spawns_nothing() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(true),
    )
    .unwrap();
    assert!(launched.client.is_none());
    assert_eq!(launched.state, SessionState::Cancelled);
    assert!(!fixture.root.join("report.json").exists());
}
#[test]
fn native_force_requires_graceful_stop_and_current_scope() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"ignore_term");
    set(&mut input, b"FIXTURE_SECONDS", b"10");
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    while !fixture.root.join("report.json").exists() {
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!client.request(native::Operation::Force).unwrap().accepted);
    assert!(!client.request(native::Operation::Cancel).unwrap().accepted);
    assert!(client.request(native::Operation::Stop).unwrap().accepted);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        store.session(launched.session).unwrap().state,
        SessionState::StopRequested
    );
    assert!(client.request(native::Operation::Force).unwrap().accepted);
    wait_state(&store, launched.session, SessionState::Stopped);
}
#[test]
fn detach_preserves_live_capture_and_reconnects_without_relaunch() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"sleep");
    set(&mut input, b"FIXTURE_SECONDS", b"10");
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    assert!(client.request(native::Operation::Detach).unwrap().accepted);
    assert_eq!(
        store.session(launched.session).unwrap().state,
        SessionState::Unknown
    );
    assert!(matches!(
        store.reserve_session(game, 1, &BaseEnvironment::default()),
        Err(rustris_core::Error::SessionBusy)
    ));
    let resumed = native::reconnect(fixture.paths.clone(), launched.session).unwrap();
    assert_eq!(
        store.session(launched.session).unwrap().state,
        SessionState::Running
    );
    assert!(resumed.request(native::Operation::Stop).unwrap().accepted);
    wait_state(&store, launched.session, SessionState::Stopped);
}
#[test]
fn wrapper_exit_does_not_end_an_adopted_new_session_child() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"wrapper");
    set(&mut input, b"FIXTURE_SECONDS", b"10");
    set(&mut input, b"FIXTURE_CHILD_SETSID", b"1");
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    let end = Instant::now() + Duration::from_secs(4);
    loop {
        let status = client
            .request(native::Operation::Status)
            .unwrap()
            .status
            .unwrap();
        if status.root_exit_code == Some(0) && status.owned_children > 0 {
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(25));
    }
    assert_eq!(
        store.session(launched.session).unwrap().state,
        SessionState::Running
    );
    assert!(client.request(native::Operation::Stop).unwrap().accepted);
    wait_state(&store, launched.session, SessionState::Stopped);
}

#[test]
fn sixty_four_mib_drains_with_bounded_memory_disk_and_visible_gaps() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"flood");
    set(&mut input, b"FIXTURE_SECONDS", b"0.5");
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    let mut gap = false;
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        match client.request(native::Operation::Logs { after: 0 }) {
            Ok(reply) => {
                let status = reply.status.unwrap();
                gap |= status.log_gap;
                assert!(status.logs.memory_peak <= 1024 * 1024);
                if status.logs.stdout_read + status.logs.stderr_read == 64 * 1024 * 1024 {
                    break;
                }
            }
            Err(_) => {
                if store.session(launched.session).unwrap().state == SessionState::Exited {
                    break;
                }
            }
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(20));
    }
    let record = wait_state(&store, launched.session, SessionState::Exited);
    let evidence: native::CompletionEvidence = serde_json::from_str(&record.detail).unwrap();
    let stats = evidence.logs;
    assert_eq!(stats.stdout_read + stats.stderr_read, 64 * 1024 * 1024);
    assert!(stats.memory_peak <= 1024 * 1024);
    assert_eq!(
        stats.memory_bytes as u64 + stats.memory_dropped,
        64 * 1024 * 1024
    );
    assert_eq!(
        stats.disk_payload
            + stats.disk_queue_dropped
            + stats.disk_limit_dropped
            + stats.disk_error_dropped,
        64 * 1024 * 1024
    );
    assert!(gap || stats.memory_dropped > 0);
    let data = fs::read(
        fixture
            .root
            .join("state/logs")
            .join(format!("{}.log", record.id)),
    )
    .unwrap();
    assert!(data.len() <= 16 * 1024 * 1024);
    assert_eq!(&data[..8], b"RSTRLOG\x01");
    let mut offset = 8;
    let mut previous = 0;
    while offset < data.len() {
        assert!(offset + 29 <= data.len());
        assert!(matches!(data[offset], 1 | 2));
        let sequence = u64::from_be_bytes(data[offset + 1..offset + 9].try_into().unwrap());
        assert!(sequence > previous);
        previous = sequence;
        assert!(u64::from_be_bytes(data[offset + 9..offset + 17].try_into().unwrap()) > 0);
        let payload =
            u32::from_be_bytes(data[offset + 25..offset + 29].try_into().unwrap()) as usize;
        offset += 29 + payload;
        assert!(offset <= data.len());
    }
}
#[test]
fn blocked_disk_writer_does_not_block_control_or_pipe_draining() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"flood");
    set(&mut input, b"FIXTURE_SECONDS", b"0.1");
    let game = store.save(None, &input).unwrap();
    let logs = fixture.root.join("state/logs");
    rustris_core::paths::private_directory(&logs).unwrap();
    let lock = fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(logs.join(".budget.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        let started = Instant::now();
        let status = client
            .request(native::Operation::Status)
            .unwrap()
            .status
            .unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
        if status.logs.stdout_read + status.logs.stderr_read == 64 * 1024 * 1024
            && status.root_exit_code.is_some()
        {
            assert!(status.logs.disk_queue_dropped > 0);
            break;
        }
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        store.session(launched.session).unwrap().state,
        SessionState::Finalizing
    );
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::Unlock).unwrap();
    wait_state(&store, launched.session, SessionState::Exited);
}

#[test]
fn stop_remains_responsive_during_output_and_disk_queue_saturation() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"stream");
    let game = store.save(None, &input).unwrap();
    let logs = fixture.root.join("state/logs");
    rustris_core::paths::private_directory(&logs).unwrap();
    let lock = fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(logs.join(".budget.lock"))
        .unwrap();
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let status = client
            .request(native::Operation::Status)
            .unwrap()
            .status
            .unwrap();
        assert!(
            !fixture.root.join("stream-complete").exists(),
            "fixture stopped producing output before saturation"
        );
        if status.logs.stdout_read >= 1024 * 1024 && status.logs.disk_queue_dropped > 0 {
            break;
        }
        assert!(Instant::now() < deadline, "disk queue did not saturate");
        std::thread::sleep(Duration::from_millis(20));
    }
    let started = Instant::now();
    assert!(client.request(native::Operation::Stop).unwrap().accepted);
    assert!(started.elapsed() < Duration::from_millis(500));
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::Unlock).unwrap();
    let record = wait_state(&store, launched.session, SessionState::Stopped);
    let evidence: native::CompletionEvidence = serde_json::from_str(&record.detail).unwrap();
    assert_eq!(evidence.root_signal, Some(15));
    assert!(evidence.logs.disk_queue_dropped > 0);
    assert!(!fixture.root.join("stream-complete").exists());
}
#[test]
fn denied_log_open_reports_loss_but_keeps_draining_and_observing_exit() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"flood");
    let game = store.save(None, &input).unwrap();
    let logs = fixture.root.join("state/logs");
    rustris_core::paths::private_directory(&logs).unwrap();
    fs::set_permissions(&logs, fs::Permissions::from_mode(0o500)).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let record = wait_state(&store, launched.session, SessionState::Exited);
    let evidence: native::CompletionEvidence = serde_json::from_str(&record.detail).unwrap();
    assert_eq!(
        evidence.logs.stdout_read + evidence.logs.stderr_read,
        64 * 1024 * 1024
    );
    assert_eq!(evidence.logs.disk_payload, 0);
    assert!(evidence.logs.disk_error_dropped > 0);
    assert!(!evidence.logs.errors.is_empty());
}

struct Prepared {
    child: std::process::Child,
    client: native::Client,
    owner: rustris_core::sessions::SessionOwner,
    socket: PathBuf,
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            // This fixture never sends Start; killing its owned helper cannot affect a game.
            self.child.kill().unwrap();
            self.child.wait().unwrap();
        }
    }
}
fn prepare(fixture: &Fixture) -> Prepared {
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &fixture.input()).unwrap();
    let (session, owner) = store
        .reserve_session(game, 1, &BaseEnvironment::default())
        .unwrap();
    let options = fixture.options();
    let child = std::process::Command::new(&options.helper)
        .arg(&fixture.paths.data)
        .arg(&fixture.paths.config)
        .arg(&options.runtime)
        .arg(&options.state)
        .arg(session.id.to_string())
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let identity = loop {
        if let Some(identity) = store.session(session.id).unwrap().helper {
            break identity;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(identity.pid, child.id());
    let socket = identity.socket.path();
    let client = native::Client::new(identity, owner.clone());
    Prepared {
        child,
        client,
        owner,
        socket,
    }
}
fn raw_request(prepared: &Prepared, body: &serde_json::Value) -> serde_json::Value {
    use std::io::{Read, Write};
    let bytes = serde_json::to_vec(body).unwrap();
    let mut stream = std::os::unix::net::UnixStream::connect(&prepared.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    let length = u32::from_be_bytes(header) as usize;
    assert!(length <= 256 * 1024);
    let mut reply = vec![0; length];
    stream.read_exact(&mut reply).unwrap();
    serde_json::from_slice(&reply).unwrap()
}
#[test]
fn protocol_rejects_unknown_pid_fields_wrong_capability_and_version() {
    let fixture = Fixture::new();
    let prepared = prepare(&fixture);
    for (version, secret, operation) in [
        (
            2,
            prepared.owner.secret(),
            serde_json::json!({"kind":"status","pid":std::process::id()}),
        ),
        (2, Uuid::new_v4(), serde_json::json!({"kind":"status"})),
        (
            1,
            prepared.owner.secret(),
            serde_json::json!({"kind":"status"}),
        ),
    ] {
        let body = serde_json::json!({"version":version,"session":prepared.owner.id(),"secret":secret,"operation":operation});
        let reply = raw_request(&prepared, &body);
        assert_eq!(reply["accepted"], false);
        assert!(
            !reply
                .to_string()
                .contains(&prepared.owner.secret().to_string())
        );
    }
    assert!(!fixture.root.join("report.json").exists());
    let status = prepared
        .client
        .request(native::Operation::Status)
        .unwrap()
        .status
        .unwrap();
    assert!(!status.spawned);
    assert!(status.client_errors >= 3);
}
#[test]
fn incomplete_peer_frame_does_not_delay_another_control_request() {
    use std::io::Write;
    let fixture = Fixture::new();
    let prepared = prepare(&fixture);
    let mut slow = std::os::unix::net::UnixStream::connect(&prepared.socket).unwrap();
    slow.write_all(&100_u32.to_be_bytes()).unwrap();
    slow.write_all(b"{").unwrap();
    let started = Instant::now();
    assert!(
        prepared
            .client
            .request(native::Operation::Status)
            .unwrap()
            .accepted
    );
    assert!(started.elapsed() < Duration::from_millis(500));
}
#[test]
fn oversized_frame_is_refused_without_waiting_for_its_body() {
    use std::io::{Read, Write};
    let fixture = Fixture::new();
    let prepared = prepare(&fixture);
    let mut stream = std::os::unix::net::UnixStream::connect(&prepared.socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    stream.write_all(&u32::MAX.to_be_bytes()).unwrap();
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    let length = u32::from_be_bytes(header) as usize;
    assert!(length <= 256 * 1024);
    let mut bytes = vec![0; length];
    stream.read_exact(&mut bytes).unwrap();
    let reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reply["accepted"], false);
    assert!(!fixture.root.join("report.json").exists());
}

fn root_subreaper() {
    static SUBREAPER: std::sync::Once = std::sync::Once::new();
    SUBREAPER.call_once(|| {
        rustix::process::set_child_subreaper(rustix::process::Pid::from_raw(1)).unwrap()
    });
}
fn pidfd(raw: u32) -> rustix::fd::OwnedFd {
    rustix::process::pidfd_open(
        rustix::process::Pid::from_raw(i32::try_from(raw).unwrap()).unwrap(),
        rustix::process::PidfdFlags::empty(),
    )
    .unwrap()
}
fn child_status(fd: &rustix::fd::OwnedFd, keep: bool) -> Option<rustix::process::WaitIdStatus> {
    use std::os::fd::AsFd;
    let mut options =
        rustix::process::WaitIdOptions::EXITED | rustix::process::WaitIdOptions::NOHANG;
    if keep {
        options |= rustix::process::WaitIdOptions::NOWAIT;
    }
    rustix::process::waitid(rustix::process::WaitId::PidFd(fd.as_fd()), options).unwrap()
}
#[test]
#[ignore = "Invoked only as an isolated child controller by controller_death_keeps_the_game_alive"]
fn fixture_controller_child() {
    use std::io::Write;
    let root = PathBuf::from(std::env::var_os("RS_CONTROLLER_FIXTURE_ROOT").unwrap());
    let game = Uuid::parse_str(&std::env::var("RS_CONTROLLER_FIXTURE_GAME").unwrap()).unwrap();
    let paths = Paths::in_root(&root.join("library"));
    let options = LaunchOptions {
        helper: PathBuf::from(env!("CARGO_BIN_EXE_rustris-session")),
        runtime: native::private_runtime(Some(&root)).unwrap(),
        state: root.join("state"),
    };
    let launched = native::launch(
        paths.clone(),
        game,
        1,
        BaseEnvironment::capture().unwrap(),
        options,
        &AtomicBool::new(false),
    )
    .unwrap();
    let record = Store::open_existing(paths)
        .unwrap()
        .session(launched.session)
        .unwrap();
    let mut ready = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(root.join("controller-ready.json"))
        .unwrap();
    ready
        .write_all(
            serde_json::json!({"session":record.id,"helper":record.helper.unwrap().pid})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    ready.sync_all().unwrap();
    if std::env::var_os("RS_CONTROLLER_WAIT_EXIT").is_some() {
        let store = Store::open_existing(Paths::in_root(&root.join("library"))).unwrap();
        wait_state(&store, launched.session, SessionState::Exited);
    } else {
        std::thread::sleep(Duration::from_secs(15));
    }
}
#[test]
fn controller_death_keeps_the_game_alive_and_duplicate_reserved() {
    root_subreaper();
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"sleep");
    set(&mut input, b"FIXTURE_SECONDS", b"10");
    let game = store.save(None, &input).unwrap();
    let mut controller = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "fixture_controller_child",
            "--nocapture",
        ])
        .env("RS_CONTROLLER_FIXTURE_ROOT", &fixture.root)
        .env("RS_CONTROLLER_FIXTURE_GAME", game.to_string())
        .env("FIXTURE_AMBIENT_TOKEN", "synthetic-never-inherit")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let controller_fd = pidfd(controller.id());
    assert!(child_status(&controller_fd, true).is_none());
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.root.join("controller-ready.json").exists()
        || !fixture.root.join("heartbeat").exists()
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("controller-ready.json")).unwrap())
            .unwrap();
    let session = Uuid::parse_str(ready["session"].as_str().unwrap()).unwrap();
    let helper_fd = pidfd(ready["helper"].as_u64().unwrap() as u32);
    rustix::process::pidfd_send_signal(&controller_fd, rustix::process::Signal::KILL).unwrap();
    controller.wait().unwrap();
    assert!(child_status(&helper_fd, true).is_none());
    let before = fs::read(fixture.root.join("heartbeat")).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    assert_ne!(fs::read(fixture.root.join("heartbeat")).unwrap(), before);
    assert!(matches!(
        store.reserve_session(game, 1, &BaseEnvironment::default()),
        Err(rustris_core::Error::SessionBusy)
    ));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["ambient_secret_present"], false);
    let client = native::reconnect(fixture.paths.clone(), session).unwrap();
    assert!(client.request(native::Operation::Stop).unwrap().accepted);
    wait_state(&store, session, SessionState::Stopped);
    let deadline = Instant::now() + Duration::from_secs(3);
    while child_status(&helper_fd, true).is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child_status(&helper_fd, false).is_some());
}
#[test]
fn helper_death_marks_unknown_and_never_releases_the_profile() {
    root_subreaper();
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"sleep");
    set(&mut input, b"FIXTURE_SECONDS", b"0.5");
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let record = store.session(launched.session).unwrap();
    let helper_fd = pidfd(record.helper.unwrap().pid);
    assert!(child_status(&helper_fd, true).is_none());
    let ready = Instant::now() + Duration::from_secs(3);
    while !fixture.root.join("report.json").exists() {
        assert!(Instant::now() < ready);
        std::thread::sleep(Duration::from_millis(10));
    }
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("report.json")).unwrap()).unwrap();
    let game_fd = pidfd(report["pid"].as_u64().unwrap() as u32);
    rustix::process::pidfd_send_signal(&helper_fd, rustix::process::Signal::KILL).unwrap();
    wait_state(&store, launched.session, SessionState::Unknown);
    assert!(native::reconnect(fixture.paths.clone(), launched.session).is_err());
    assert!(matches!(
        store.reserve_session(game, 1, &BaseEnvironment::default()),
        Err(rustris_core::Error::SessionBusy)
    ));
    // Game survival after helper death is not promised; this fixture ends within 0.5s.
    let finished = Instant::now() + Duration::from_secs(3);
    while child_status(&game_fd, true).is_none() {
        assert!(Instant::now() < finished);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(child_status(&game_fd, false).is_some());
}
#[test]
fn native_executable_and_working_directory_preserve_non_utf8_paths() {
    use std::os::unix::{ffi::OsStringExt, fs::symlink};
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    let executable = fixture
        .root
        .join(std::ffi::OsString::from_vec(b"python_\xff".to_vec()));
    symlink("/usr/bin/python3", &executable).unwrap();
    let cwd = fixture
        .root
        .join(std::ffi::OsString::from_vec(b"cwd_\xfe".to_vec()));
    fs::DirBuilder::new().mode(0o700).create(&cwd).unwrap();
    input.executable = OsBytes::new(executable.as_os_str().as_bytes()).unwrap();
    input.cwd = OsBytes::new(cwd.as_os_str().as_bytes()).unwrap();
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    wait_state(&store, launched.session, SessionState::Exited);
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("report.json")).unwrap()).unwrap();
    let expected = cwd
        .as_os_str()
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(report["cwd"], expected);
    assert_eq!(
        store
            .session(launched.session)
            .unwrap()
            .snapshot
            .source
            .profile,
        input
    );
}

#[test]
fn failed_stop_storage_never_signals_and_recovery_reverifies_ownership() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"sleep");
    set(&mut input, b"FIXTURE_SECONDS", b"10");
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let client = launched.client.unwrap();
    let ready = Instant::now() + Duration::from_secs(3);
    while !fixture.root.join("heartbeat").exists() {
        assert!(Instant::now() < ready);
        std::thread::sleep(Duration::from_millis(10));
    }
    // The controller holds a real SQLite writer lock, not a synthetic failure flag.
    let connection = rusqlite_fixture_lock(&fixture);
    let rejected = client.request(native::Operation::Stop).unwrap();
    assert!(!rejected.accepted);
    let before = fs::read(fixture.root.join("heartbeat")).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    assert_ne!(fs::read(fixture.root.join("heartbeat")).unwrap(), before);
    connection.execute_batch("ROLLBACK").unwrap();
    let restored = native::reconnect(fixture.paths.clone(), launched.session).unwrap();
    assert!(restored.request(native::Operation::Stop).unwrap().accepted);
    let record = wait_state(&store, launched.session, SessionState::Stopped);
    let evidence: native::CompletionEvidence = serde_json::from_str(&record.detail).unwrap();
    assert_eq!(evidence.root_signal, Some(15));
}
fn rusqlite_fixture_lock(fixture: &Fixture) -> rusqlite::Connection {
    let connection = rusqlite::Connection::open(fixture.paths.database()).unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    connection
}

#[test]
fn actual_enospc_capture_keeps_draining_and_records_the_failure() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"flood");
    let game = store.save(None, &input).unwrap();
    let state = fixture.root.join("state");
    rustris_core::paths::private_directory(&state).unwrap();
    let capture = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(fixture.root.join("namespace.log"))
        .unwrap();
    let mut child = std::process::Command::new("bwrap")
        .args([
            "--die-with-parent",
            "--unshare-net",
            "--unshare-pid",
            "--ro-bind",
            "/",
            "/",
            "--bind",
        ])
        .arg(&fixture.root)
        .arg(&fixture.root)
        .args([
            "--dev", "/dev", "--proc", "/proc", "--perms", "0700", "--size", "65536", "--tmpfs",
        ])
        .arg(&state)
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "fixture_controller_child",
            "--nocapture",
        ])
        .env("RS_CONTROLLER_FIXTURE_ROOT", &fixture.root)
        .env("RS_CONTROLLER_FIXTURE_GAME", game.to_string())
        .env("RS_CONTROLLER_WAIT_EXIT", "1")
        .stdout(capture.try_clone().unwrap())
        .stderr(capture)
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "{}",
                fs::read_to_string(fixture.root.join("namespace.log")).unwrap()
            );
            break;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("The ENOSPC fixture did not finish");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let ready: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("controller-ready.json")).unwrap())
            .unwrap();
    let session = Uuid::parse_str(ready["session"].as_str().unwrap()).unwrap();
    let record = store.session(session).unwrap();
    assert_eq!(record.state, SessionState::Exited);
    let evidence: native::CompletionEvidence = serde_json::from_str(&record.detail).unwrap();
    assert_eq!(
        evidence.logs.stdout_read + evidence.logs.stderr_read,
        64 * 1024 * 1024
    );
    assert!(evidence.logs.disk_error_dropped > 0);
    assert!(
        evidence
            .logs
            .errors
            .iter()
            .any(|message| message.contains("os error 28"))
    );
}
#[test]
fn stop_does_not_signal_an_unrelated_direct_child() {
    let fixture = Fixture::new();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let mut input = fixture.input();
    set(&mut input, b"FIXTURE_MODE", b"sleep");
    set(&mut input, b"FIXTURE_SECONDS", b"10");
    let game = store.save(None, &input).unwrap();
    let mut sentinel = std::process::Command::new("/usr/bin/python3")
        .args(["-c", "import time; time.sleep(10)"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let fd = pidfd(sentinel.id());
    assert!(child_status(&fd, true).is_none());
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        launched
            .client
            .unwrap()
            .request(native::Operation::Stop)
            .unwrap()
            .accepted
    );
    wait_state(&store, launched.session, SessionState::Stopped);
    assert!(child_status(&fd, true).is_none());
    rustix::process::pidfd_send_signal(&fd, rustix::process::Signal::TERM).unwrap();
    sentinel.wait().unwrap();
}

#[test]
fn completed_reader_rejects_live_locked_and_malformed_frames() {
    use std::io::Write;
    let fixture = Fixture::new();
    let session = Uuid::new_v4();
    let log = fixture.root.join(format!("{session}.log"));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(&log)
        .unwrap();
    rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive).unwrap();
    assert!(native::read_completed(&fixture.root, session).is_err());
    rustix::fs::flock(&file, rustix::fs::FlockOperation::Unlock).unwrap();
    file.write_all(b"RSTRLOG\x01").unwrap();
    file.write_all(&[1]).unwrap();
    assert!(
        native::read_completed(&fixture.root, session)
            .unwrap_err()
            .to_string()
            .contains("complete log frame")
    );
    file.set_len(8).unwrap();
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(&[1]).unwrap();
    file.write_all(&1_u64.to_be_bytes()).unwrap();
    file.write_all(&123_u64.to_be_bytes()).unwrap();
    file.write_all(&0_u64.to_be_bytes()).unwrap();
    file.write_all(&u32::MAX.to_be_bytes()).unwrap();
    assert!(
        native::read_completed(&fixture.root, session)
            .unwrap_err()
            .to_string()
            .contains("frame length")
    );
}

#[test]
#[ignore = "Only invoked as a bounded child by the FIFO regression"]
fn completed_fifo_reader_child() {
    let root = PathBuf::from(std::env::var_os("RUSTRIS_FIFO_TEST_ROOT").unwrap());
    let session = Uuid::parse_str(&std::env::var("RUSTRIS_FIFO_TEST_SESSION").unwrap()).unwrap();
    assert!(native::read_completed(&root, session).is_err());
}

#[test]
fn completed_reader_rejects_fifo_without_waiting_for_a_writer() {
    let fixture = Fixture::new();
    let session = Uuid::new_v4();
    let path = fixture.root.join(format!("{session}.log"));
    rustix::fs::mknodat(
        rustix::fs::CWD,
        &path,
        rustix::fs::FileType::Fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        0,
    )
    .unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "completed_fifo_reader_child"])
        .env("RUSTRIS_FIFO_TEST_ROOT", &fixture.root)
        .env("RUSTRIS_FIFO_TEST_SESSION", session.to_string())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let outcome = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        outcome.is_some_and(|status| status.success()),
        "FIFO reader blocked or failed"
    );
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_fifo());
}

#[test]
fn completed_reader_bounds_tail_and_preserves_stream_time_and_bytes() {
    use std::io::Write;
    let fixture = Fixture::new();
    let session = Uuid::new_v4();
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(fixture.root.join(format!("{session}.log")))
        .unwrap();
    file.write_all(b"RSTRLOG\x01").unwrap();
    for sequence in 1..=12_u64 {
        file.write_all(&[if sequence % 2 == 0 { 1 } else { 2 }])
            .unwrap();
        file.write_all(&sequence.to_be_bytes()).unwrap();
        file.write_all(&(123 + sequence).to_be_bytes()).unwrap();
        file.write_all(&((sequence - 1) * 32768).to_be_bytes())
            .unwrap();
        file.write_all(&32768_u32.to_be_bytes()).unwrap();
        file.write_all(&vec![0xff; 32768]).unwrap();
    }
    drop(file);
    let tail = native::read_completed(&fixture.root, session).unwrap();
    let held = fs::File::open(fixture.root.join(format!("{session}.log"))).unwrap();
    assert!(
        rustix::fs::flock(&held, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_err()
    );
    let records = &tail.records;
    assert_eq!(records.len(), 4);
    assert_eq!(records[0].sequence, 9);
    assert_eq!(records[0].stream, 2);
    assert_eq!(records[0].timestamp_us, 132);
    assert_eq!(records[0].offset, 8 * 32768);
    assert_eq!(records[0].data, vec![0xff; 32768]);
    drop(tail);
    rustix::fs::flock(&held, rustix::fs::FlockOperation::NonBlockingLockExclusive).unwrap();
}

#[test]
fn native_preflight_distinguishes_inaccessible_from_missing_and_wrong_type() {
    let fixture = Fixture::new();
    let mut input = fixture.input();
    input.executable =
        OsBytes::new(fixture.root.join("fixture.py").as_os_str().as_bytes()).unwrap();
    input.cwd = input.executable.clone();
    let mut store = Store::open(fixture.paths.clone()).unwrap();
    let game = store.save(None, &input).unwrap();
    let launched = native::launch(
        fixture.paths.clone(),
        game,
        1,
        BaseEnvironment::default(),
        fixture.options(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(launched.state, SessionState::LaunchFailed);
    assert!(launched.client.is_none());
    assert_eq!(launched.preflight.observations[0].outcome, "inaccessible");
    assert_eq!(launched.preflight.observations[0].errno, Some(13));
    assert_eq!(launched.preflight.observations[1].outcome, "wrong_type");
    assert_eq!(launched.preflight.findings.len(), 2);
    for finding in &launched.preflight.findings {
        assert!(
            launched
                .preflight
                .observations
                .iter()
                .any(|o| o.id == finding.evidence && !o.source.is_empty() && o.observed_ms > 0)
        );
    }
    assert!(store.session(launched.session).unwrap().helper.is_none());
}
