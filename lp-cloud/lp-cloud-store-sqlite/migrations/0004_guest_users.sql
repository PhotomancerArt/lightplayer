-- Guest accounts (examples vision D8): browser-held ownership, minted for
-- an anonymous fork's publish. The flag is THE pruning lever — "which rows
-- are guest-owned" must stay one obvious query — so it is a real column
-- (mirrored by provider = 'anonymous'), not an inference from provider
-- text. Existing rows are all real sign-ins: default 0.
ALTER TABLE users ADD COLUMN anonymous INTEGER NOT NULL DEFAULT 0;

-- The pruning query's entry point: guest users (and, joined through
-- projects.owner, everything they own).
CREATE INDEX idx_users_anonymous ON users (anonymous) WHERE anonymous = 1;
