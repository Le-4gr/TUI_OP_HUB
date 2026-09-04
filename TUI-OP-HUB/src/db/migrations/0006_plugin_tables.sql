-- Plugin/mod system (US-PLG-05/06/10). Replaces the unused stub tables from
-- 0001 (the plugin feature was never functional; those tables are empty),
-- giving the runtime the schema it needs.
DROP TABLE IF EXISTS plugin_approvals;
DROP TABLE IF EXISTS plugins;

CREATE TABLE IF NOT EXISTS plugins (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    version      TEXT NOT NULL,
    plugin_type  TEXT NOT NULL DEFAULT 'lua',
    capabilities TEXT NOT NULL DEFAULT '[]',
    enabled      INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS plugin_approvals (
    plugin_id  TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    user_id    TEXT NOT NULL,
    approved   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (plugin_id, user_id)
);
