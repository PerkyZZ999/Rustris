CREATE TABLE session_runners (
    session_id BLOB PRIMARY KEY NOT NULL REFERENCES launch_sessions(id),
    executable BLOB NOT NULL CHECK(typeof(executable)='blob'),
    proton BLOB NOT NULL CHECK(typeof(proton)='blob'),
    runtime BLOB NOT NULL CHECK(typeof(runtime)='blob'),
    umu_version TEXT NOT NULL CHECK(length(umu_version)<=4096),
    proton_version TEXT NOT NULL CHECK(length(proton_version)<=4096),
    runtime_version TEXT NOT NULL CHECK(length(runtime_version)<=4096),
    policy TEXT NOT NULL CHECK(policy='umu-prepared-v1'),
    prefix_path BLOB NOT NULL CHECK(typeof(prefix_path)='blob'),
    prefix_device BLOB NOT NULL CHECK(typeof(prefix_device)='blob' AND length(prefix_device)=8),
    prefix_inode BLOB NOT NULL CHECK(typeof(prefix_inode)='blob' AND length(prefix_inode)=8)
);
CREATE TRIGGER frozen_runner_insert BEFORE INSERT ON session_runners
WHEN (SELECT sealed FROM launch_sessions WHERE id=NEW.session_id)=1
OR (SELECT runner FROM launch_sessions WHERE id=NEW.session_id)!='umu'
BEGIN SELECT RAISE(ABORT,'Runner snapshots require an unsealed UMU session'); END;
CREATE TRIGGER frozen_runner_update BEFORE UPDATE ON session_runners BEGIN SELECT RAISE(ABORT,'Runner snapshots are immutable'); END;
CREATE TRIGGER frozen_runner_delete BEFORE DELETE ON session_runners BEGIN SELECT RAISE(ABORT,'Runner snapshots are immutable'); END;

CREATE TABLE session_preflights (
    session_id BLOB NOT NULL,
    generation INTEGER NOT NULL,
    report TEXT NOT NULL CHECK(length(CAST(report AS BLOB))<=32768),
    PRIMARY KEY(session_id,generation),
    FOREIGN KEY(session_id,generation) REFERENCES session_events(session_id,generation)
);
CREATE TRIGGER preflight_preparing BEFORE INSERT ON session_preflights
WHEN (SELECT state FROM session_events WHERE session_id=NEW.session_id AND generation=NEW.generation)!='preparing'
BEGIN SELECT RAISE(ABORT,'Preflight evidence requires a preparing event'); END;
CREATE TRIGGER frozen_preflight_update BEFORE UPDATE ON session_preflights BEGIN SELECT RAISE(ABORT,'Preflight evidence is append-only'); END;
CREATE TRIGGER frozen_preflight_delete BEFORE DELETE ON session_preflights BEGIN SELECT RAISE(ABORT,'Preflight evidence is append-only'); END;
