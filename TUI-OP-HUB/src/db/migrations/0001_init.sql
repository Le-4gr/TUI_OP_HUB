-- Migration: 0001_init
-- Initial schema for TUI-OP-HUB

CREATE TABLE IF NOT EXISTS types (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT
);

CREATE TABLE IF NOT EXISTS tags (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS entities (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    description   TEXT,
    content       TEXT,
    type_id       TEXT NOT NULL REFERENCES types(id),
    project_id    TEXT REFERENCES projects(id) ON DELETE SET NULL,
    metadata_json TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS entity_tags (
    entity_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    tag_id    TEXT NOT NULL REFERENCES tags(id)    ON DELETE CASCADE,
    PRIMARY KEY (entity_id, tag_id)
);

CREATE VIRTUAL TABLE IF NOT EXISTS entities_fts USING fts5(
    name, description, content,
    content='entities', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS entities_ai AFTER INSERT ON entities BEGIN
    INSERT INTO entities_fts(rowid, name, description, content)
    VALUES (new.rowid, new.name, new.description, new.content);
END;

CREATE TRIGGER IF NOT EXISTS entities_ad AFTER DELETE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, name, description, content)
    VALUES ('delete', old.rowid, old.name, old.description, old.content);
END;

CREATE TRIGGER IF NOT EXISTS entities_au AFTER UPDATE ON entities BEGIN
    INSERT INTO entities_fts(entities_fts, rowid, name, description, content)
    VALUES ('delete', old.rowid, old.name, old.description, old.content);
    INSERT INTO entities_fts(rowid, name, description, content)
    VALUES (new.rowid, new.name, new.description, new.content);
END;

INSERT OR IGNORE INTO types (id, name, description) VALUES
    ('cmd', 'command', 'A shell command'),
    ('script', 'script', 'A multi-line script'),
    ('app', 'app', 'An application'),
    ('wf', 'workflow', 'An automation workflow'),
    ('env', 'environment', 'An environment definition'),
    ('cfg', 'config', 'A configuration file'),
    ('sec', 'secret', 'A secret reference'),
    ('proj', 'project', 'A project resource');

-- User profiles and keys
CREATE TABLE IF NOT EXISTS user_profiles (
    id          TEXT PRIMARY KEY,
    username    TEXT NOT NULL UNIQUE,
    email       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS user_keys (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL UNIQUE REFERENCES user_profiles(id) ON DELETE CASCADE,
    key_b64     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Secrets storage (encrypted values)
CREATE TABLE IF NOT EXISTS secrets (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES user_profiles(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    value_enc   TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, name)
);

-- Plugin manifests and runs
CREATE TABLE IF NOT EXISTS plugins (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    version     TEXT NOT NULL,
    manifest    TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS plugin_approvals (
    id          TEXT PRIMARY KEY,
    plugin_id   TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    action      TEXT NOT NULL,
    status      TEXT NOT NULL,
    requester   TEXT,
    approver    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- SSH hosts
CREATE TABLE IF NOT EXISTS ssh_hosts (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    hostname    TEXT NOT NULL,
    port        INTEGER DEFAULT 22,
    username    TEXT,
    key_path    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Scheduled tasks
CREATE TABLE IF NOT EXISTS scheduled_tasks (
    id          TEXT PRIMARY KEY,
    workflow_id TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    cron_expr   TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    last_run    TEXT,
    next_run    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Workflow run history
CREATE TABLE IF NOT EXISTS workflow_runs (
    run_id         TEXT PRIMARY KEY,
    workflow_id    TEXT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    success        INTEGER NOT NULL,
    output         TEXT,
    error          TEXT,
    duration_ms    INTEGER,
    steps_completed INTEGER,
    created_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
