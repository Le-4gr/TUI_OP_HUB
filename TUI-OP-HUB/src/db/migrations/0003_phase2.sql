-- Migration: 0003_phase2
-- Phase 2 foundations (US-SEC admin, US-SEC-01..05, US-ENV, US-SSH-01..06)

-- First registered user becomes admin; admins manage users
ALTER TABLE user_profiles ADD COLUMN is_admin INTEGER DEFAULT 0;

-- Secret classification and access control
ALTER TABLE secrets ADD COLUMN secret_kind TEXT DEFAULT 'password';
ALTER TABLE secrets ADD COLUMN requires_reauth INTEGER DEFAULT 0;

-- Projects as launch pads: environment + editor integration (US-ENV)
ALTER TABLE projects ADD COLUMN env_type TEXT;
ALTER TABLE projects ADD COLUMN env_cmd TEXT;
