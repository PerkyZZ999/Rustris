CREATE TABLE local_artwork (
    game_id BLOB NOT NULL PRIMARY KEY REFERENCES games(id) ON DELETE CASCADE,
    path BLOB NOT NULL CHECK(typeof(path)='blob' AND length(path)>0 AND length(path)<=65536)
);
