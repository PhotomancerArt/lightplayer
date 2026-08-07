-- 0001_initial: every table the cloud service needs.
--
-- Text columns hold the canonical text form of the domain's types: a
-- PrefixedUid is `prj_h7Kq9xY2mQ4tB8Wz`, a ContentHash is 64 lowercase hex
-- characters. Storing them as text costs a few bytes and buys a database a
-- human can read with `sqlite3` during an incident, which at this scale is
-- the better trade.
--
-- Timestamps are REAL f64 epoch seconds — the same type the clock port
-- hands the domain, so nothing is rounded on the way in or out.
--
-- Every child table cascades from its project. Nothing in the service
-- deletes a project yet; the cascade is here so that when something does,
-- it cannot leave a project's events behind as unreachable rows.

CREATE TABLE users (
    uid          TEXT PRIMARY KEY NOT NULL,
    google_sub   TEXT NOT NULL,
    email        TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at   REAL NOT NULL
);

-- Not UNIQUE, deliberately: identity is the uid, and the two secondary
-- lookups are indexes rather than constraints so this adapter accepts
-- exactly the writes the in-memory one accepts (the conformance suite is
-- the contract). The domain upserts an account by Google subject, so two
-- rows never share a subject in practice.
CREATE INDEX users_by_google_sub ON users (google_sub);
CREATE INDEX users_by_email ON users (email);

-- Sessions are stored by the SHA-256 of the bearer token, never the token
-- itself: a stolen database dump must not be a stack of live cookies.
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY NOT NULL,
    user_uid   TEXT NOT NULL REFERENCES users (uid) ON DELETE CASCADE,
    expires_at REAL NOT NULL
);

CREATE INDEX sessions_by_user ON sessions (user_uid);

CREATE TABLE projects (
    uid        TEXT PRIMARY KEY NOT NULL,
    -- RESTRICT, not CASCADE: deleting an account must never silently take
    -- its projects (and everyone else's access to them) with it. That is a
    -- deliberate migration, not a foreign-key side effect.
    owner_uid  TEXT NOT NULL REFERENCES users (uid) ON DELETE RESTRICT,
    visibility TEXT NOT NULL,
    slug       TEXT NOT NULL,
    created_at REAL NOT NULL
);

CREATE INDEX projects_by_owner ON projects (owner_uid);

-- Membership is project x email. `user_uid` is NULL while the invitation is
-- pending: an email that has never logged in gets a row that grants nothing
-- until first login resolves it (Q4).
CREATE TABLE members (
    project_uid TEXT NOT NULL REFERENCES projects (uid) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    user_uid    TEXT REFERENCES users (uid) ON DELETE SET NULL,
    role        TEXT NOT NULL,
    added_at    REAL NOT NULL,
    PRIMARY KEY (project_uid, email)
);

CREATE INDEX members_by_user ON members (user_uid);

-- The one index that makes first login cheap: resolving pending rows scans
-- only the unresolved ones.
CREATE INDEX members_pending_by_email ON members (email) WHERE user_uid IS NULL;

-- The head frontier: one row per head, so more than one head is an ordinary
-- state rather than an encoding trick. `ordinal` preserves the order the
-- frontier was written in, which is what makes a read give back exactly what
-- was stored.
--
-- `parents` is a JSON array of tree hashes. It is read and written whole —
-- no query ever filters on a single parent — so a child table would buy
-- nothing but joins.
CREATE TABLE refs (
    project_uid TEXT    NOT NULL REFERENCES projects (uid) ON DELETE CASCADE,
    ordinal     INTEGER NOT NULL,
    tree        TEXT    NOT NULL,
    parents     TEXT    NOT NULL,
    PRIMARY KEY (project_uid, ordinal)
);

-- `seq` is a server ordinal, 1-based and monotonic within a project. The
-- primary key is what makes a gap or a duplicate impossible rather than
-- merely unlikely.
CREATE TABLE events (
    project_uid TEXT    NOT NULL REFERENCES projects (uid) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,
    json        TEXT    NOT NULL,
    PRIMARY KEY (project_uid, seq)
);

-- Client-computed display metadata, stored verbatim as the JSON the client
-- pushed (D3). The server never opens project content to derive or correct
-- it, so it gets no columns of its own to be wrong about.
CREATE TABLE sidecars (
    project_uid TEXT PRIMARY KEY NOT NULL REFERENCES projects (uid) ON DELETE CASCADE,
    json        TEXT NOT NULL
);

-- Which blobs the service holds. The bytes live in a BlobStore; this is the
-- index push validation reads inside the same transaction as the rest of a
-- push, which is why it is a MetaStore table and not a BlobStore method.
CREATE TABLE blob_index (
    hash TEXT    PRIMARY KEY NOT NULL,
    size INTEGER NOT NULL
);
