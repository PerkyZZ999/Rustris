-- D-010: the fingerprint of the launch review the user last approved for a game.
CREATE TABLE approved_reviews (
    game_id BLOB NOT NULL PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    fingerprint BLOB NOT NULL CHECK(typeof(fingerprint)='blob' AND length(fingerprint)=32),
    approved_ms INTEGER NOT NULL
);
-- D-011: artwork Rustris downloaded into its own data directory. A user-chosen
-- local_artwork reference always takes precedence over these files.
CREATE TABLE fetched_artwork (
    game_id BLOB NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('cover','hero')),
    path BLOB NOT NULL CHECK(typeof(path)='blob' AND length(path)>0 AND length(path)<=65536),
    source TEXT NOT NULL CHECK(length(source) BETWEEN 1 AND 64),
    fetched_ms INTEGER NOT NULL,
    PRIMARY KEY(game_id, kind)
);
