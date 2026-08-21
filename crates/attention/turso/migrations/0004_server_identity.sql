CREATE TABLE attention_server_identity (
    singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
    server_id TEXT NOT NULL UNIQUE,
    stream_id TEXT NOT NULL UNIQUE
);
