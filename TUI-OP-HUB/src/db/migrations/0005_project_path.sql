-- US-PROJ: remember where a project's workspace lives on disk so it can be
-- opened in an editor later. NULL = no directory registered yet.
ALTER TABLE projects ADD COLUMN path TEXT;
