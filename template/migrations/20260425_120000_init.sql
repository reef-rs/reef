-- Initial schema. The `schema_migrations` table tracks which files have been
-- applied; `cargo reef migrate run` (and the runner inside cargo-reef) rely
-- on its presence.
--
-- Generate further migrations with `cargo reef migrate new <name>`.

CREATE TABLE IF NOT EXISTS schema_migrations (
    name TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS greeting (
    id INTEGER PRIMARY KEY,
    text TEXT NOT NULL
);

INSERT OR IGNORE INTO greeting (id, text) VALUES (1, 'hello from libSQL');
