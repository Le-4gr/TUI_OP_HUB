-- Migration: Add user authentication support
-- US-SEC-11, US-SEC-12, US-SEC-13, US-SEC-14, US-SEC-15

-- Add authentication columns to user_profiles
ALTER TABLE user_profiles ADD COLUMN password_hash TEXT;
ALTER TABLE user_profiles ADD COLUMN salt TEXT;
ALTER TABLE user_profiles ADD COLUMN auth_method TEXT DEFAULT 'env' CHECK(auth_method IN ('env', 'password'));
-- Note: created_at already exists from 0001_init.sql
ALTER TABLE user_profiles ADD COLUMN last_login TEXT;
ALTER TABLE user_profiles ADD COLUMN session_timeout_minutes INTEGER DEFAULT 15;

-- Create audit log table for security events
CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL, -- 'login_success', 'login_failure', 'password_change', 'secret_access', 'plugin_install'
    event_data TEXT, -- JSON with additional details
    ip_address TEXT,
    timestamp TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES user_profiles(id) ON DELETE CASCADE
);

CREATE INDEX idx_audit_log_user_id ON audit_log(user_id);
CREATE INDEX idx_audit_log_timestamp ON audit_log(timestamp);
CREATE INDEX idx_audit_log_event_type ON audit_log(event_type);

-- Create command history table
CREATE TABLE IF NOT EXISTS command_history (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    entity_id TEXT, -- Reference to command/script entity
    command_text TEXT NOT NULL,
    executed_at TEXT DEFAULT CURRENT_TIMESTAMP,
    exit_code INTEGER,
    duration_ms INTEGER,
    FOREIGN KEY (user_id) REFERENCES user_profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE SET NULL
);

CREATE INDEX idx_command_history_user_id ON command_history(user_id);
CREATE INDEX idx_command_history_executed_at ON command_history(executed_at);

-- Create favorites table
CREATE TABLE IF NOT EXISTS favorites (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    added_at TEXT DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES user_profiles(id) ON DELETE CASCADE,
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE,
    UNIQUE(user_id, entity_id)
);

CREATE INDEX idx_favorites_user_id ON favorites(user_id);

-- Add execution count to entities for statistics
ALTER TABLE entities ADD COLUMN execution_count INTEGER DEFAULT 0;
ALTER TABLE entities ADD COLUMN last_executed_at TEXT;
