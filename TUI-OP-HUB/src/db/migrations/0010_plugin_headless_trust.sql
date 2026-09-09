-- Plugin approval workflow for headless service runs (US-PLG leftover):
-- admins can mark an approved plugin as trusted for headless mode, so the
-- background service auto-approves it on startup without interactive consent.
ALTER TABLE plugin_approvals ADD COLUMN trusted_headless INTEGER NOT NULL DEFAULT 0;
