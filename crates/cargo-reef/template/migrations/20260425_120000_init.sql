-- Initial schema for the reef-template app.
--
-- The `schema_migrations` tracking table is auto-created by `cargo reef
-- migrate run` — don't define it here.
--
-- Generate further migrations with `cargo reef migrate new <name>`.

CREATE TABLE IF NOT EXISTS greeting (
    id INTEGER PRIMARY KEY,
    text TEXT NOT NULL
);

INSERT OR IGNORE INTO greeting (id, text) VALUES (1, 'hello from libSQL');
