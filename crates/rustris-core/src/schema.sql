CREATE TABLE games (
    id BLOB NOT NULL PRIMARY KEY CHECK(typeof(id)='blob' AND length(id)=16),
    display_name TEXT NOT NULL,
    favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1))
);
CREATE TABLE installations (
    id BLOB NOT NULL PRIMARY KEY CHECK(typeof(id)='blob' AND length(id)=16),
    game_id BLOB NOT NULL UNIQUE REFERENCES games(id) ON DELETE CASCADE
);
CREATE TABLE profiles (
    id BLOB NOT NULL PRIMARY KEY CHECK(typeof(id)='blob' AND length(id)=16),
    installation_id BLOB NOT NULL UNIQUE REFERENCES installations(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL DEFAULT 1 CHECK(typeof(revision)='integer' AND revision>0),
    executable BLOB NOT NULL CHECK(typeof(executable)='blob'),
    cwd BLOB NOT NULL CHECK(typeof(cwd)='blob'),
    runner TEXT NOT NULL CHECK(runner IN ('native','umu')),
    prefix BLOB CHECK(prefix IS NULL OR typeof(prefix)='blob')
);
CREATE TABLE arguments (
    profile_id BLOB NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position>=0),
    value BLOB NOT NULL CHECK(typeof(value)='blob'),
    PRIMARY KEY(profile_id,position)
);
CREATE TABLE environment (
    profile_id BLOB NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position>=0),
    key BLOB NOT NULL CHECK(typeof(key)='blob'),
    value BLOB CHECK(value IS NULL OR typeof(value)='blob'),
    PRIMARY KEY(profile_id,position),
    UNIQUE(profile_id,key)
);
CREATE TABLE ui_state (
    id INTEGER PRIMARY KEY CHECK(id=1),
    selected_game BLOB REFERENCES games(id) ON DELETE SET NULL
);
INSERT INTO ui_state(id) VALUES(1);
