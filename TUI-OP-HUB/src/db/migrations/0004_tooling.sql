-- Migration: 0004_tooling
-- Command families with structured options + seeded knowledge base

-- Command families: options/flags are child entities of a parent command
ALTER TABLE entities ADD COLUMN parent_id TEXT REFERENCES entities(id) ON DELETE CASCADE;

-- New entity type for command options
INSERT OR IGNORE INTO types (id, name, description) VALUES
    ('opt', 'option', 'A command option/flag with description');

-- Seed marker table (idempotent prepopulation)
CREATE TABLE IF NOT EXISTS seed_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
