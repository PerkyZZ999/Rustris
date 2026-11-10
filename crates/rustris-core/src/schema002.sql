CREATE TABLE launch_sessions (
    id BLOB NOT NULL PRIMARY KEY CHECK(typeof(id)='blob' AND length(id)=16),
    game_id BLOB NOT NULL CHECK(length(game_id)=16),
    installation_id BLOB NOT NULL CHECK(length(installation_id)=16),
    profile_id BLOB NOT NULL CHECK(length(profile_id)=16),
    revision INTEGER NOT NULL CHECK(typeof(revision)='integer' AND revision>0),
    display_name TEXT NOT NULL,
    runner TEXT NOT NULL CHECK(runner IN ('native','umu')),
    executable BLOB NOT NULL CHECK(typeof(executable)='blob'),
    cwd BLOB NOT NULL CHECK(typeof(cwd)='blob'),
    prefix BLOB CHECK(prefix IS NULL OR typeof(prefix)='blob'),
    environment_policy TEXT NOT NULL,
    sealed INTEGER NOT NULL DEFAULT 0 CHECK(sealed IN (0,1)),
    state TEXT NOT NULL CHECK(state IN ('preparing','running','stop_requested','force_requested','finalizing','exited','stopped','cancelled','launch_failed','unknown','released_unknown')),
    generation INTEGER NOT NULL DEFAULT 0 CHECK(typeof(generation)='integer' AND generation>=0),
    created_ms INTEGER NOT NULL,
    completed_ms INTEGER,
    exit_code INTEGER,
    detail TEXT NOT NULL DEFAULT '',
    secret BLOB NOT NULL CHECK(typeof(secret)='blob' AND length(secret)=16),
    helper_pid INTEGER,
    helper_start_ticks INTEGER,
    helper_boot_id BLOB,
    socket BLOB
);
CREATE TABLE session_arguments (
    session_id BLOB NOT NULL REFERENCES launch_sessions(id),
    position INTEGER NOT NULL CHECK(position>=0),
    value BLOB NOT NULL CHECK(typeof(value)='blob'),
    PRIMARY KEY(session_id,position)
);
CREATE TABLE session_environment (
    session_id BLOB NOT NULL REFERENCES launch_sessions(id),
    position INTEGER NOT NULL CHECK(position>=0),
    key BLOB NOT NULL CHECK(typeof(key)='blob'),
    value BLOB NOT NULL CHECK(typeof(value)='blob'),
    PRIMARY KEY(session_id,position),
    UNIQUE(session_id,key)
);
CREATE TABLE session_environment_delta (
    session_id BLOB NOT NULL REFERENCES launch_sessions(id),
    position INTEGER NOT NULL CHECK(position>=0),
    key BLOB NOT NULL CHECK(typeof(key)='blob'),
    value BLOB CHECK(value IS NULL OR typeof(value)='blob'),
    PRIMARY KEY(session_id,position),
    UNIQUE(session_id,key)
);
CREATE TABLE session_reservations (
    session_id BLOB NOT NULL PRIMARY KEY REFERENCES launch_sessions(id),
    profile_id BLOB NOT NULL UNIQUE,
    prefix_path BLOB UNIQUE,
    prefix_device BLOB,
    prefix_inode BLOB,
    CHECK((prefix_path IS NULL AND prefix_device IS NULL AND prefix_inode IS NULL) OR
          (typeof(prefix_path)='blob' AND typeof(prefix_device)='blob' AND length(prefix_device)=8 AND typeof(prefix_inode)='blob' AND length(prefix_inode)=8)),
    UNIQUE(prefix_device,prefix_inode)
);
CREATE TABLE session_events (
    session_id BLOB NOT NULL REFERENCES launch_sessions(id),
    generation INTEGER NOT NULL,
    happened_ms INTEGER NOT NULL,
    state TEXT NOT NULL,
    detail TEXT NOT NULL,
    PRIMARY KEY(session_id,generation)
);
CREATE TRIGGER frozen_session BEFORE UPDATE OF game_id,installation_id,profile_id,revision,display_name,runner,executable,cwd,prefix,environment_policy,sealed,created_ms,secret ON launch_sessions
WHEN OLD.sealed=1 BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_arguments_insert BEFORE INSERT ON session_arguments
WHEN (SELECT sealed FROM launch_sessions WHERE id=NEW.session_id)=1 BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_arguments_update BEFORE UPDATE ON session_arguments BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_arguments_delete BEFORE DELETE ON session_arguments BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_environment_insert BEFORE INSERT ON session_environment
WHEN (SELECT sealed FROM launch_sessions WHERE id=NEW.session_id)=1 BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_environment_update BEFORE UPDATE ON session_environment BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_environment_delete BEFORE DELETE ON session_environment BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER reserved_profile_update BEFORE UPDATE ON profiles
WHEN EXISTS(SELECT 1 FROM session_reservations WHERE profile_id=OLD.id) BEGIN SELECT RAISE(ABORT,'This profile has an active or unknown session'); END;
CREATE TRIGGER reserved_profile_delete BEFORE DELETE ON profiles
WHEN EXISTS(SELECT 1 FROM session_reservations WHERE profile_id=OLD.id) BEGIN SELECT RAISE(ABORT,'This profile has an active or unknown session'); END;

CREATE TRIGGER frozen_delta_insert BEFORE INSERT ON session_environment_delta
WHEN (SELECT sealed FROM launch_sessions WHERE id=NEW.session_id)=1 BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_delta_update BEFORE UPDATE ON session_environment_delta BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER frozen_delta_delete BEFORE DELETE ON session_environment_delta BEGIN SELECT RAISE(ABORT,'Launch snapshots are immutable'); END;
CREATE TRIGGER immutable_session_events_update BEFORE UPDATE ON session_events BEGIN SELECT RAISE(ABORT,'Session events are append-only'); END;
CREATE TRIGGER immutable_session_events_delete BEFORE DELETE ON session_events BEGIN SELECT RAISE(ABORT,'Session events are append-only'); END;
