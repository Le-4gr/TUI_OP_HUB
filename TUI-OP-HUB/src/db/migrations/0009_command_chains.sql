-- Command chains (US-CMD): pipe/semicolon one-liners as first-class entities
-- (e.g. `cat /proc/meminfo | grep Dirty`), runnable like plain commands.
INSERT OR IGNORE INTO types (id, name, description) VALUES
    ('chain', 'command chain', 'A piped/semicolon command chain one-liner');
