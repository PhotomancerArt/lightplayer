//! Every piece of service state, in one SQLite file.
//!
//! # How the connection is opened
//!
//! Four pragmas, and each one is a decision:
//!
//! - **`journal_mode = WAL`**. Readers do not block the writer and the
//!   writer does not block readers, which is what lets a project listing
//!   render while a push is committing. It is also what makes continuous
//!   replication possible at all: Litestream (P10) ships the WAL, so
//!   without this there is no backup story.
//! - **`synchronous = NORMAL`**. In WAL mode this fsyncs at checkpoints
//!   rather than at every commit. The exposure is the last fraction of a
//!   second of commits *if the machine loses power* — not if the process
//!   dies, which WAL already survives. That is the right trade for a
//!   service whose clients hold the authoritative copy of their own
//!   history and re-push what the server missed. `FULL` would cost a
//!   round-trip to the disk on every save.
//! - **`foreign_keys = ON`**. SQLite defaults this *off*, which means the
//!   `REFERENCES` clauses in the schema would be decorative. They are not
//!   decorative: they are what stops a project's events from outliving the
//!   project. Note the consequence — this adapter **refuses an orphan**
//!   (events for a project that was never stored) where the in-memory one
//!   accepts it. The service always writes the parent first.
//! - **`busy_timeout`**. One writer is the design, but a WAL checkpoint or
//!   a replication reader can hold a lock for a moment; waiting briefly is
//!   better than a fatal "database is locked".
//!
//! # Why one `Connection` and no pool
//!
//! One process, one writer, and the port's `&mut self` methods already
//! serialize writes at the type level. A pool would add contention on a
//! single-writer file and a class of "which connection am I in a
//! transaction on" bug, in exchange for nothing.
//!
//! # What failure does
//!
//! Nothing here returns a `Result`: see [`crate::store_fatal`]. A SQLite
//! failure at this scale is a node-level fault, and the recovery path is
//! the supervisor plus a Litestream restore.

use std::path::Path;

use lp_cloud_domain::{
    CloudProject, CloudUser, HeadRef, MemberRecord, MemberRole, MetaStore, ProjectRefs,
    SessionRecord, StoredEvent,
};
use lpc_cloud_api::{SidecarMeta, Visibility};
use lpc_history::{ContentHash, HistoryEvent, PrefixedUid};
use rusqlite::{Connection, Params, Row, params};

use crate::migrations::run_migrations;
use crate::store_fatal::fatal;

/// The service's state in a SQLite database.
///
/// Open one with [`open`](SqliteMetaStore::open) (a file, which is what a
/// deployment uses) or [`open_in_memory`](SqliteMetaStore::open_in_memory)
/// (a private database that vanishes with the value, which is what tests
/// use). Either way the schema is migrated on the way out of the
/// constructor, so there is no "did you remember to migrate" state.
#[derive(Debug)]
pub struct SqliteMetaStore {
    conn: Connection,
}

impl SqliteMetaStore {
    /// Open (creating if needed) the database at `path`, apply pragmas, and
    /// migrate it to the current schema.
    pub fn open(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let conn = fatal(
            &format!("opening the database at {}", path.display()),
            Connection::open(path),
        );
        Self::from_connection(conn)
    }

    /// Open a private in-memory database, migrated and ready.
    ///
    /// WAL is meaningless for an in-memory database (SQLite keeps its
    /// memory journal), which is the one way this differs from
    /// [`open`](SqliteMetaStore::open) — every statement the store runs is
    /// otherwise identical, so the conformance suite exercises the same SQL
    /// either way.
    pub fn open_in_memory() -> Self {
        let conn = fatal(
            "opening an in-memory database",
            Connection::open_in_memory(),
        );
        Self::from_connection(conn)
    }

    /// Apply the connection pragmas and migrate.
    fn from_connection(mut conn: Connection) -> Self {
        fatal(
            "applying the connection pragmas",
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;\n\
                 PRAGMA synchronous = NORMAL;\n\
                 PRAGMA foreign_keys = ON;\n\
                 PRAGMA busy_timeout = 5000;",
            ),
        );
        fatal("migrating the schema", run_migrations(&mut conn));
        Self { conn }
    }

    /// The `user_version` this database is at.
    pub fn schema_version(&self) -> u32 {
        fatal(
            "reading the schema version",
            self.conn
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0)),
        ) as u32
    }

    // ---- statement helpers -------------------------------------------

    /// Run a statement, reporting how many rows it changed.
    fn execute(&self, operation: &str, sql: &str, params: impl Params) -> usize {
        fatal(operation, self.conn.execute(sql, params))
    }

    /// Read at most one row.
    fn query_one<T>(
        &self,
        operation: &str,
        sql: &str,
        params: impl Params,
        decode: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Option<T> {
        fatal(
            operation,
            self.conn
                .query_row(sql, params, decode)
                .map(Some)
                .or_else(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                }),
        )
    }

    /// Read every row.
    fn query_all<T>(
        &self,
        operation: &str,
        sql: &str,
        params: impl Params,
        decode: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Vec<T> {
        fatal(operation, self.try_query_all(sql, params, decode))
    }

    fn try_query_all<T>(
        &self,
        sql: &str,
        params: impl Params,
        decode: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> rusqlite::Result<Vec<T>> {
        let mut statement = self.conn.prepare(sql)?;
        let rows = statement.query_map(params, decode)?;
        rows.collect()
    }

    /// Replace a project's frontier: delete the old rows and write the new
    /// ones in one transaction, so a reader never sees half a frontier.
    fn try_put_refs(&mut self, project: PrefixedUid, refs: &ProjectRefs) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "DELETE FROM refs WHERE project_uid = ?1",
            params![project.to_string()],
        )?;
        for (ordinal, head) in refs.heads.iter().enumerate() {
            tx.execute(
                "INSERT INTO refs (project_uid, ordinal, tree, parents) VALUES (?1, ?2, ?3, ?4)",
                params![
                    project.to_string(),
                    ordinal as i64,
                    head.tree.to_string(),
                    encode_hashes(&head.parents),
                ],
            )?;
        }
        tx.commit()
    }

    /// Append events, numbering them from the log's current end. One
    /// transaction: the read of `MAX(seq)` and the inserts that depend on it
    /// cannot be interleaved with another append.
    fn try_append_events(
        &mut self,
        project: PrefixedUid,
        events: &[HistoryEvent],
    ) -> rusqlite::Result<u64> {
        let tx = self.conn.transaction()?;
        let mut seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM events WHERE project_uid = ?1",
            params![project.to_string()],
            |row| row.get(0),
        )?;
        for event in events {
            seq += 1;
            tx.execute(
                "INSERT INTO events (project_uid, seq, json) VALUES (?1, ?2, ?3)",
                params![project.to_string(), seq, encode_json(event)],
            )?;
        }
        tx.commit()?;
        Ok(seq as u64)
    }
}

impl MetaStore for SqliteMetaStore {
    // ---- users -------------------------------------------------------

    fn put_user(&mut self, user: CloudUser) {
        // An upsert, not `INSERT OR REPLACE`: replace *deletes* the row
        // first, and with foreign keys on that would cascade a returning
        // user's sessions away underneath them.
        self.execute(
            "MetaStore::put_user",
            "INSERT INTO users\n\
                 (uid, google_sub, email, display_name, given_name, family_name, picture_url, provider, created_at)\n\
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)\n\
             ON CONFLICT (uid) DO UPDATE SET\n\
                 google_sub = excluded.google_sub,\n\
                 email = excluded.email,\n\
                 display_name = excluded.display_name,\n\
                 given_name = excluded.given_name,\n\
                 family_name = excluded.family_name,\n\
                 picture_url = excluded.picture_url,\n\
                 provider = excluded.provider,\n\
                 created_at = excluded.created_at",
            params![
                user.uid.to_string(),
                user.google_sub,
                user.email,
                user.display_name,
                user.given_name,
                user.family_name,
                user.picture_url,
                user.provider,
                user.created_at,
            ],
        );
    }

    fn user(&self, uid: PrefixedUid) -> Option<CloudUser> {
        self.query_one(
            "MetaStore::user",
            "SELECT uid, google_sub, email, display_name, given_name, family_name, picture_url, provider, created_at\n\
             FROM users WHERE uid = ?1",
            params![uid.to_string()],
            decode_user,
        )
    }

    fn user_by_google_sub(&self, google_sub: &str) -> Option<CloudUser> {
        self.query_one(
            "MetaStore::user_by_google_sub",
            "SELECT uid, google_sub, email, display_name, given_name, family_name, picture_url, provider, created_at\n\
             FROM users WHERE google_sub = ?1 ORDER BY rowid DESC LIMIT 1",
            params![google_sub],
            decode_user,
        )
    }

    fn user_by_email(&self, email: &str) -> Option<CloudUser> {
        self.query_one(
            "MetaStore::user_by_email",
            "SELECT uid, google_sub, email, display_name, given_name, family_name, picture_url, provider, created_at\n\
             FROM users WHERE email = ?1 ORDER BY rowid DESC LIMIT 1",
            params![email],
            decode_user,
        )
    }

    fn users(&self, limit: usize) -> Vec<CloudUser> {
        self.query_all(
            "MetaStore::users",
            "SELECT uid, google_sub, email, display_name, given_name, family_name, picture_url, provider, created_at\n\
             FROM users ORDER BY created_at, uid LIMIT ?1",
            params![limit as i64],
            decode_user,
        )
    }

    // ---- sessions ----------------------------------------------------

    fn put_session(&mut self, session: SessionRecord) {
        self.execute(
            "MetaStore::put_session",
            "INSERT INTO sessions (token_hash, user_uid, created_at, expires_at, user_agent)\n\
             VALUES (?1, ?2, ?3, ?4, ?5)\n\
             ON CONFLICT (token_hash) DO UPDATE SET\n\
                 user_uid = excluded.user_uid,\n\
                 created_at = excluded.created_at,\n\
                 expires_at = excluded.expires_at,\n\
                 user_agent = excluded.user_agent",
            params![
                session.token_hash.to_string(),
                session.user.to_string(),
                session.created_at,
                session.expires_at,
                session.user_agent,
            ],
        );
    }

    fn session(&self, token_hash: ContentHash) -> Option<SessionRecord> {
        self.query_one(
            "MetaStore::session",
            "SELECT token_hash, user_uid, created_at, expires_at, user_agent\n\
             FROM sessions WHERE token_hash = ?1",
            params![token_hash.to_string()],
            decode_session,
        )
    }

    fn delete_session(&mut self, token_hash: ContentHash) {
        self.execute(
            "MetaStore::delete_session",
            "DELETE FROM sessions WHERE token_hash = ?1",
            params![token_hash.to_string()],
        );
    }

    fn sessions_for_user(&self, user: PrefixedUid) -> Vec<SessionRecord> {
        self.query_all(
            "MetaStore::sessions_for_user",
            "SELECT token_hash, user_uid, created_at, expires_at, user_agent\n\
             FROM sessions WHERE user_uid = ?1 ORDER BY created_at, token_hash",
            params![user.to_string()],
            decode_session,
        )
    }

    // ---- projects ----------------------------------------------------

    fn put_project(&mut self, project: CloudProject) {
        // Upsert for the same reason as `put_user`, and here it is load
        // bearing: every child table cascades from `projects`, so an
        // `INSERT OR REPLACE` on a visibility change would delete the
        // project's members, refs, events and sidecar.
        self.execute(
            "MetaStore::put_project",
            "INSERT INTO projects (uid, owner_uid, visibility, slug, created_at)\n\
             VALUES (?1, ?2, ?3, ?4, ?5)\n\
             ON CONFLICT (uid) DO UPDATE SET\n\
                 owner_uid = excluded.owner_uid,\n\
                 visibility = excluded.visibility,\n\
                 slug = excluded.slug,\n\
                 created_at = excluded.created_at",
            params![
                project.uid.to_string(),
                project.owner.to_string(),
                visibility_to_text(project.visibility),
                project.slug,
                project.created_at,
            ],
        );
    }

    fn project(&self, uid: PrefixedUid) -> Option<CloudProject> {
        self.query_one(
            "MetaStore::project",
            "SELECT uid, owner_uid, visibility, slug, created_at FROM projects WHERE uid = ?1",
            params![uid.to_string()],
            decode_project,
        )
    }

    fn projects_for_user(&self, user: PrefixedUid) -> Vec<CloudProject> {
        self.query_all(
            "MetaStore::projects_for_user",
            "SELECT p.uid, p.owner_uid, p.visibility, p.slug, p.created_at\n\
             FROM members m JOIN projects p ON p.uid = m.project_uid\n\
             WHERE m.user_uid = ?1\n\
             ORDER BY m.project_uid, m.email",
            params![user.to_string()],
            decode_project,
        )
    }

    // ---- membership --------------------------------------------------

    fn put_member(&mut self, member: MemberRecord) {
        self.execute(
            "MetaStore::put_member",
            "INSERT INTO members (project_uid, email, user_uid, role, added_at)\n\
             VALUES (?1, ?2, ?3, ?4, ?5)\n\
             ON CONFLICT (project_uid, email) DO UPDATE SET\n\
                 user_uid = excluded.user_uid,\n\
                 role = excluded.role,\n\
                 added_at = excluded.added_at",
            params![
                member.project.to_string(),
                member.email,
                member.user.map(|uid| uid.to_string()),
                role_to_text(member.role),
                member.added_at,
            ],
        );
    }

    fn remove_member(&mut self, project: PrefixedUid, email: &str) -> bool {
        self.execute(
            "MetaStore::remove_member",
            "DELETE FROM members WHERE project_uid = ?1 AND email = ?2",
            params![project.to_string(), email],
        ) > 0
    }

    fn members(&self, project: PrefixedUid) -> Vec<MemberRecord> {
        self.query_all(
            "MetaStore::members",
            "SELECT project_uid, email, user_uid, role, added_at FROM members\n\
             WHERE project_uid = ?1 ORDER BY email",
            params![project.to_string()],
            decode_member,
        )
    }

    fn member_for_user(&self, project: PrefixedUid, user: PrefixedUid) -> Option<MemberRecord> {
        self.query_one(
            "MetaStore::member_for_user",
            "SELECT project_uid, email, user_uid, role, added_at FROM members\n\
             WHERE project_uid = ?1 AND user_uid = ?2 ORDER BY email LIMIT 1",
            params![project.to_string(), user.to_string()],
            decode_member,
        )
    }

    fn resolve_pending_members(&mut self, email: &str, user: PrefixedUid) -> usize {
        self.execute(
            "MetaStore::resolve_pending_members",
            "UPDATE members SET user_uid = ?2 WHERE email = ?1 AND user_uid IS NULL",
            params![email, user.to_string()],
        )
    }

    // ---- refs / heads ------------------------------------------------

    fn refs(&self, project: PrefixedUid) -> ProjectRefs {
        ProjectRefs {
            heads: self.query_all(
                "MetaStore::refs",
                "SELECT tree, parents FROM refs WHERE project_uid = ?1 ORDER BY ordinal",
                params![project.to_string()],
                decode_head,
            ),
        }
    }

    fn put_refs(&mut self, project: PrefixedUid, refs: ProjectRefs) {
        fatal("MetaStore::put_refs", self.try_put_refs(project, &refs));
    }

    // ---- sidecars ----------------------------------------------------

    fn sidecar(&self, project: PrefixedUid) -> Option<SidecarMeta> {
        self.query_one(
            "MetaStore::sidecar",
            "SELECT json FROM sidecars WHERE project_uid = ?1",
            params![project.to_string()],
            decode_sidecar,
        )
    }

    fn put_sidecar(&mut self, project: PrefixedUid, sidecar: SidecarMeta) {
        self.execute(
            "MetaStore::put_sidecar",
            "INSERT INTO sidecars (project_uid, json) VALUES (?1, ?2)\n\
             ON CONFLICT (project_uid) DO UPDATE SET json = excluded.json",
            params![project.to_string(), encode_json(&sidecar)],
        );
    }

    // ---- event log ---------------------------------------------------

    fn append_events(&mut self, project: PrefixedUid, events: &[HistoryEvent]) -> u64 {
        fatal(
            "MetaStore::append_events",
            self.try_append_events(project, events),
        )
    }

    fn events(&self, project: PrefixedUid) -> Vec<StoredEvent> {
        self.query_all(
            "MetaStore::events",
            "SELECT seq, json FROM events WHERE project_uid = ?1 ORDER BY seq",
            params![project.to_string()],
            decode_event,
        )
    }

    fn events_since(&self, project: PrefixedUid, since: u64) -> Vec<StoredEvent> {
        self.query_all(
            "MetaStore::events_since",
            "SELECT seq, json FROM events WHERE project_uid = ?1 AND seq > ?2 ORDER BY seq",
            params![project.to_string(), since as i64],
            decode_event,
        )
    }

    fn last_event_seq(&self, project: PrefixedUid) -> u64 {
        self.query_one(
            "MetaStore::last_event_seq",
            "SELECT COALESCE(MAX(seq), 0) FROM events WHERE project_uid = ?1",
            params![project.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0) as u64
    }

    // ---- blob index --------------------------------------------------

    fn has_blob(&self, hash: ContentHash) -> bool {
        self.blob_size(hash).is_some()
    }

    fn record_blob(&mut self, hash: ContentHash, size: u64) {
        self.execute(
            "MetaStore::record_blob",
            "INSERT INTO blob_index (hash, size) VALUES (?1, ?2)\n\
             ON CONFLICT (hash) DO UPDATE SET size = excluded.size",
            params![hash.to_string(), size as i64],
        );
    }

    fn blob_size(&self, hash: ContentHash) -> Option<u64> {
        self.query_one(
            "MetaStore::blob_size",
            "SELECT size FROM blob_index WHERE hash = ?1",
            params![hash.to_string()],
            |row| row.get::<_, i64>(0),
        )
        .map(|size| size as u64)
    }
}

// ---- row decoding -----------------------------------------------------
//
// A row that will not decode is a corrupt database, not a request that
// failed: these die by the same rule as a failed statement.

fn decode_user(row: &Row<'_>) -> rusqlite::Result<CloudUser> {
    Ok(CloudUser {
        uid: parse_uid(&row.get::<_, String>(0)?),
        google_sub: row.get(1)?,
        email: row.get(2)?,
        display_name: row.get(3)?,
        given_name: row.get(4)?,
        family_name: row.get(5)?,
        picture_url: row.get(6)?,
        provider: row.get(7)?,
        created_at: row.get(8)?,
    })
}

fn decode_session(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        token_hash: parse_hash(&row.get::<_, String>(0)?),
        user: parse_uid(&row.get::<_, String>(1)?),
        created_at: row.get(2)?,
        expires_at: row.get(3)?,
        user_agent: row.get(4)?,
    })
}

fn decode_project(row: &Row<'_>) -> rusqlite::Result<CloudProject> {
    Ok(CloudProject {
        uid: parse_uid(&row.get::<_, String>(0)?),
        owner: parse_uid(&row.get::<_, String>(1)?),
        visibility: visibility_from_text(&row.get::<_, String>(2)?),
        slug: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn decode_member(row: &Row<'_>) -> rusqlite::Result<MemberRecord> {
    Ok(MemberRecord {
        project: parse_uid(&row.get::<_, String>(0)?),
        email: row.get(1)?,
        user: row
            .get::<_, Option<String>>(2)?
            .map(|text| parse_uid(&text)),
        role: role_from_text(&row.get::<_, String>(3)?),
        added_at: row.get(4)?,
    })
}

fn decode_head(row: &Row<'_>) -> rusqlite::Result<HeadRef> {
    Ok(HeadRef {
        tree: parse_hash(&row.get::<_, String>(0)?),
        parents: decode_hashes(&row.get::<_, String>(1)?),
    })
}

fn decode_event(row: &Row<'_>) -> rusqlite::Result<StoredEvent> {
    Ok(StoredEvent {
        seq: row.get::<_, i64>(0)? as u64,
        event: decode_json(&row.get::<_, String>(1)?),
    })
}

fn decode_sidecar(row: &Row<'_>) -> rusqlite::Result<SidecarMeta> {
    Ok(decode_json(&row.get::<_, String>(0)?))
}

// ---- column encodings -------------------------------------------------

/// Pinned to the wire spelling of [`Visibility`], so a row reads the same
/// as the API that produced it.
fn visibility_to_text(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Private => "private",
        Visibility::Link => "link",
    }
}

fn visibility_from_text(text: &str) -> Visibility {
    match text {
        "private" => Visibility::Private,
        "link" => Visibility::Link,
        other => panic!("lp-cloud-store-sqlite: unknown visibility {other:?} in the database"),
    }
}

fn role_to_text(role: MemberRole) -> &'static str {
    match role {
        MemberRole::Owner => "owner",
        MemberRole::Member => "member",
    }
}

fn role_from_text(text: &str) -> MemberRole {
    match text {
        "owner" => MemberRole::Owner,
        "member" => MemberRole::Member,
        other => panic!("lp-cloud-store-sqlite: unknown member role {other:?} in the database"),
    }
}

fn encode_hashes(hashes: &[ContentHash]) -> String {
    fatal("encoding a hash list", serde_json::to_string(hashes))
}

fn decode_hashes(json: &str) -> Vec<ContentHash> {
    decode_json(json)
}

fn encode_json<T: serde::Serialize>(value: &T) -> String {
    fatal("encoding a JSON column", serde_json::to_string(value))
}

fn decode_json<T: serde::de::DeserializeOwned>(json: &str) -> T {
    fatal("decoding a JSON column", serde_json::from_str(json))
}

fn parse_uid(text: &str) -> PrefixedUid {
    fatal("decoding a uid column", text.parse())
}

fn parse_hash(text: &str) -> ContentHash {
    fatal("decoding a content-hash column", text.parse())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_history::UidPrefix;
    use tempfile::tempdir;

    /// The behaviour a `BTreeMap` cannot check for us: a store written,
    /// dropped, and reopened is the same store.
    #[test]
    fn state_survives_reopening_the_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("cloud.sqlite3");
        let uid = PrefixedUid::mint(UidPrefix::User, &[7; 16]);

        {
            let mut store = SqliteMetaStore::open(&path);
            store.put_user(CloudUser {
                uid,
                google_sub: "g-7".into(),
                email: "seven@example.com".into(),
                display_name: "Seven".into(),
                given_name: None,
                family_name: None,
                picture_url: None,
                provider: "google".into(),
                created_at: 7.5,
            });
        }

        let store = SqliteMetaStore::open(&path);
        let user = store.user(uid).expect("the user survived the reopen");
        assert_eq!(user.email, "seven@example.com");
        assert_eq!(user.created_at, 7.5);
        assert_eq!(store.schema_version(), crate::migrations::latest_version());
    }

    /// The pragmas are the posture; a silently-ignored one would cost us
    /// crash safety or replication with nothing to show for it.
    #[test]
    fn a_file_database_is_in_wal_mode_with_foreign_keys_on() {
        let dir = tempdir().unwrap();
        let store = SqliteMetaStore::open(dir.path().join("cloud.sqlite3"));

        let journal_mode: String = store
            .conn
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");

        let foreign_keys: i64 = store
            .conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);

        let synchronous: i64 = store
            .conn
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1, "synchronous should be NORMAL");
    }

    /// Foreign keys are on, so this adapter refuses what the in-memory one
    /// tolerates. Pinned as a test because it is the one place the two
    /// stores deliberately differ, and a caller that trips it is writing a
    /// child before its parent.
    #[test]
    #[should_panic(expected = "MetaStore::put_sidecar")]
    fn writing_a_child_row_without_its_project_is_fatal() {
        let mut store = SqliteMetaStore::open_in_memory();
        store.put_sidecar(
            PrefixedUid::mint(UidPrefix::Project, &[1; 16]),
            SidecarMeta {
                name: "orphan".into(),
                format_version: 4,
                preview_png: None,
            },
        );
    }
}
