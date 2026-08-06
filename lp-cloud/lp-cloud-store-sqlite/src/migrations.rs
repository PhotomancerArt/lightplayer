//! Hand-numbered SQL migrations, applied by the `user_version` pragma.
//!
//! # Why `user_version` and not a table
//!
//! SQLite carries a 32-bit integer in its file header that belongs to the
//! application, and it is exactly the state a migration runner needs: the
//! number of migrations applied. Reading it is a pragma rather than a query
//! against a table that might itself not exist yet, so there is no
//! bootstrap step and no "create the migrations table if missing" branch to
//! get wrong.
//!
//! # The rules
//!
//! - Migrations are **numbered and immutable**. `0001_initial.sql` is what
//!   shipped; a change to the schema is `0002_…`, never an edit.
//! - Each migration runs in **its own transaction**, which also bumps
//!   `user_version`. SQLite makes DDL transactional, so a migration that
//!   fails halfway leaves the database exactly as it was — the runner stops
//!   at the failure and the version still names the last migration that
//!   fully applied.
//! - Applying is **idempotent**: a database already at version N skips the
//!   first N and applies the rest, so the same call opens a fresh file and
//!   an existing one.

use core::fmt;

use rusqlite::Connection;

/// Every migration, in order. The index in this slice (1-based) is the
/// `user_version` a database has once that migration has been applied.
const MIGRATIONS: &[Migration] = &[Migration {
    name: "0001_initial",
    sql: include_str!("../migrations/0001_initial.sql"),
}];

/// Bring a database up to the current schema and report the version it
/// ended at.
pub fn run_migrations(conn: &mut Connection) -> Result<u32, MigrationError> {
    apply(conn, MIGRATIONS)
}

/// The schema version a fully-migrated database is at.
pub fn latest_version() -> u32 {
    MIGRATIONS.len() as u32
}

/// One numbered migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// The file's name, which is what an error names.
    pub name: &'static str,
    /// Its SQL, embedded at compile time — the binary carries its own
    /// schema and cannot be deployed next to the wrong migrations
    /// directory.
    pub sql: &'static str,
}

/// A migration failed, and which one.
///
/// The name matters more than the SQLite error: "0004_add_folders failed"
/// tells an operator which deploy to roll back, and the database is still
/// at the version before it.
#[derive(Debug)]
pub struct MigrationError {
    /// The migration that failed, or `user_version` if reading the version
    /// itself did.
    pub migration: &'static str,
    /// What SQLite said.
    pub source: rusqlite::Error,
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "migration {} failed: {}", self.migration, self.source)
    }
}

impl std::error::Error for MigrationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Apply the migrations in `migrations` that this database has not seen.
///
/// Split out from [`run_migrations`] so the tests can drive a doctored list
/// — in particular one whose second migration fails, which is the case that
/// decides whether a half-applied schema is possible.
fn apply(conn: &mut Connection, migrations: &[Migration]) -> Result<u32, MigrationError> {
    let mut version = schema_version(conn)?;
    for (index, migration) in migrations.iter().enumerate() {
        let target = index as u32 + 1;
        if target <= version {
            continue;
        }
        run_one(conn, migration, target).map_err(|source| MigrationError {
            migration: migration.name,
            source,
        })?;
        version = target;
    }
    Ok(version)
}

/// One migration, in one transaction that also bumps the version.
fn run_one(conn: &mut Connection, migration: &Migration, target: u32) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(migration.sql)?;
    tx.pragma_update(None, "user_version", target)?;
    tx.commit()
}

/// The `user_version` this database is at (0 for a fresh file).
fn schema_version(conn: &Connection) -> Result<u32, MigrationError> {
    conn.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))
        .map(|version| version as u32)
        .map_err(|source| MigrationError {
            migration: "user_version",
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_database_gets_the_whole_schema() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 0);

        assert_eq!(run_migrations(&mut conn).unwrap(), latest_version());
        assert_eq!(schema_version(&conn).unwrap(), latest_version());
        for table in [
            "users",
            "sessions",
            "projects",
            "members",
            "refs",
            "events",
            "sidecars",
            "blob_index",
        ] {
            assert!(table_exists(&conn, table), "missing table {table}");
        }
    }

    #[test]
    fn running_an_already_migrated_database_is_a_no_op() {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO blob_index (hash, size) VALUES ('deadbeef', 3)",
            [],
        )
        .unwrap();

        // The second run must not re-run `CREATE TABLE` (which would error)
        // and must not lose the row.
        assert_eq!(run_migrations(&mut conn).unwrap(), latest_version());
        let size: i64 = conn
            .query_row("SELECT size FROM blob_index", [], |row| row.get(0))
            .unwrap();
        assert_eq!(size, 3);
    }

    /// The case that decides whether a half-applied schema is reachable: the
    /// second migration fails, and the database must be left at version 1
    /// with nothing of migration 2 in it.
    #[test]
    fn a_failing_migration_leaves_the_previous_version_intact() {
        let doctored = &[
            Migration {
                name: "0001_first",
                sql: "CREATE TABLE first (id INTEGER PRIMARY KEY);",
            },
            Migration {
                name: "0002_broken",
                sql: "CREATE TABLE second (id INTEGER PRIMARY KEY);\n\
                      CREATE TABLE second (id INTEGER PRIMARY KEY);",
            },
        ];

        let mut conn = Connection::open_in_memory().unwrap();
        assert!(apply(&mut conn, doctored).is_err());

        assert_eq!(schema_version(&conn).unwrap(), 1);
        assert!(table_exists(&conn, "first"));
        assert!(
            !table_exists(&conn, "second"),
            "the failed migration's first statement was not rolled back"
        );
    }

    /// A database that stopped partway (version 1 of 2) picks up where it
    /// left off rather than starting over.
    #[test]
    fn a_partially_migrated_database_applies_only_what_is_missing() {
        let first = Migration {
            name: "0001_first",
            sql: "CREATE TABLE first (id INTEGER PRIMARY KEY);",
        };
        let second = Migration {
            name: "0002_second",
            sql: "CREATE TABLE second (id INTEGER PRIMARY KEY);",
        };

        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(apply(&mut conn, &[first]).unwrap(), 1);
        assert_eq!(apply(&mut conn, &[first, second]).unwrap(), 2);
        assert!(table_exists(&conn, "first"));
        assert!(table_exists(&conn, "second"));
    }

    fn table_exists(conn: &Connection, table: &str) -> bool {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            > 0
    }
}
