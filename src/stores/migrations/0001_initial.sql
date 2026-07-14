-- Diagnosis run history. SQLite stores uuid as BLOB and timestamps as TEXT
-- (it has no native uuid/timestamp types). Summary columns are denormalized so
-- `list` works without parsing the report blob, even across report versions.
CREATE TABLE runs (
    id BLOB PRIMARY KEY NOT NULL,
    saved_at TEXT NOT NULL,
    model_name TEXT,
    client_mode TEXT NOT NULL,
    health TEXT NOT NULL,
    fired_count INTEGER NOT NULL,
    report_version INTEGER NOT NULL,
    report TEXT NOT NULL
);
