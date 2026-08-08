-- 0002_profile_and_sessions: account profile columns, session metadata, and
-- the provider a user account was created through.
--
-- `provider` defaults every existing row to 'google': every account this
-- database could already hold predates the dev picker's provider-labeling
-- need, and was in practice created through Google sign-in (dev auth stamps
-- its own `google_sub` prefix but nothing distinguished it in the schema
-- until now). A fresh account write always sets it explicitly — see
-- `CloudService::upsert_user`.
--
-- `sessions.created_at` defaults existing rows to 0 (epoch), which is
-- honest about what the database actually knows: nothing recorded when
-- those sessions opened. They still sort first (oldest) in `ListSessions`,
-- which is a reasonable place for a session this migration cannot date to
-- land.

ALTER TABLE users ADD COLUMN given_name TEXT;
ALTER TABLE users ADD COLUMN family_name TEXT;
ALTER TABLE users ADD COLUMN picture_url TEXT;
ALTER TABLE users ADD COLUMN provider TEXT NOT NULL DEFAULT 'google';

ALTER TABLE sessions ADD COLUMN created_at REAL NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN user_agent TEXT;
