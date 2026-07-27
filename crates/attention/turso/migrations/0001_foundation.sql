CREATE TABLE __attention_migrations (
    version INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL UNIQUE,
    checksum BLOB NOT NULL CHECK(length(checksum) = 32)
);

CREATE TABLE __attention_probe (
    operation_id TEXT PRIMARY KEY NOT NULL,
    fingerprint BLOB NOT NULL,
    value BLOB NOT NULL
);
