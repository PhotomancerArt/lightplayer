# lp-cloud-store-sqlite

The persistent half of the cloud service: one SQLite file for state, a
directory (or a bucket) for bytes.

Three adapters over the `lp-cloud-domain` ports:

| Adapter | Port | What it is |
| --- | --- | --- |
| `SqliteMetaStore` | `MetaStore` | Every piece of service state in one SQLite file |
| `FsBlobStore` | `BlobStore` | Content-addressed files under a directory — the default |
| `S3BlobStore` | `BlobStore` | The same bytes in an S3-compatible bucket (Tigris), behind the `s3` feature |

## Why SQLite, and what the pragmas buy

One process, one writer, one file to back up. At the scale this service is
planned for, a database server would be a second thing to operate in
exchange for nothing.

Four pragmas are applied on every connection, and each is a decision:

- **`journal_mode = WAL`** — readers do not block the writer, so a project
  listing renders while a push commits. It is also the precondition for
  replication: Litestream ships the WAL.
- **`synchronous = NORMAL`** — in WAL mode this fsyncs at checkpoints
  rather than every commit. The exposure is the last fraction of a second
  of commits *on power loss*, not on a process crash (WAL already survives
  that). Clients hold the authoritative copy of their own history and
  re-push what the server missed, so `FULL`'s disk round-trip per save
  would be paid for a risk we have already covered.
- **`foreign_keys = ON`** — SQLite defaults this *off*, which would make
  the schema's `REFERENCES` clauses decorative. They are load bearing:
  they are what stops a project's events from outliving the project.
- **`busy_timeout = 5000`** — a WAL checkpoint or a replication reader can
  hold a lock for a moment. Waiting briefly beats a fatal "database is
  locked".

No connection pool: the port's `&mut self` write methods already serialize
writes, and a pool over a single-writer file adds contention and a class of
"which connection is my transaction on" bug in exchange for nothing.

## Litestream is the backup story (P10)

Nothing in this crate configures it — that is deployment. What this crate
owes it is WAL mode, which is above, and the discipline of keeping all
state in the one file so that "restore the database" is a complete
recovery rather than a partial one. Blobs are content-addressed and
immutable, so they are backed up by whatever the object store already does
(or by copying the directory).

## Failure is fatal, on purpose

`MetaStore` and `BlobStore` are **infallible ports** — the domain's error
vocabulary is what a client gets told, and there is nothing useful to tell
a client about a disk that stopped answering. So the backend-failure policy
lives here, and it is to **panic with the operation named**:

```
lp-cloud-store-sqlite: MetaStore::append_events failed: database is locked
```

Recovery is the process supervisor restarting onto a Litestream restore.
Threading a `Result` up through the domain would only produce a handler
that answers "500" — while the one genuinely worse outcome, continuing past
a failed write and acknowledging a push whose events were never stored, is
exactly what stopping prevents. `src/store_fatal.rs` is where this is
written down.

## Migrations

`migrations/000N_*.sql`, embedded with `include_str!` so the binary carries
its own schema, applied by `src/migrations.rs` against the `user_version`
pragma. Each migration runs in its own transaction that also bumps the
version, and SQLite makes DDL transactional — so a migration that fails
halfway leaves the database exactly as it was, at the version of the last
one that fully applied.

Migrations are numbered and **immutable**: a schema change is `0002_…`,
never an edit to `0001_initial.sql`.

## The conformance suite is the contract

`tests/store_conformance.rs` runs one battery of checks against four
adapters: the in-memory `MetaStore`/`BlobStore` from `lp-cloud-store-mem`
and the SQLite/filesystem ones here. The checks are written once, against
`&mut dyn MetaStore` and `&mut dyn BlobStore`.

This is the crate's most important test, because every layer above the
ports is tested against the in-memory adapters. The moment the fake answers
a question differently from SQLite, all of those tests are asserting
something that is not true in production — a data-corruption bug that keeps
the suite green.

The two deliberately differ in exactly one way, pinned by its own test:
**foreign keys are enforced here**, so writing a child row (a member, a
sidecar, an event) before its project is fatal, where the in-memory store
accepts it. The service always writes the parent first.

## The S3 adapter's async bridge

`object_store` is async and `BlobStore` is not, so `S3BlobStore` owns a
private current-thread tokio runtime and does one `block_on` per operation.

That is the smallest bridge that actually works. A hand-rolled `block_on`
would drive the future and nothing else — `object_store`'s S3 backend runs
on reqwest/hyper, whose sockets are registered with a tokio *reactor*, so
with no reactor the first poll returns `Pending` and is never woken. It
would not be a smaller bridge; it would be a hang. Growing an async variant
of the port instead would push `async fn` into a domain-facing trait for
the benefit of one adapter.

**Callers must not invoke it from inside an async task** — `block_on`
panics on a thread already running a runtime. An async server edge reaches
it through `tokio::task::spawn_blocking`, which is what it should do with a
blocking store anyway.

There is no local test of the S3 adapter: a hand-written S3 fake would only
prove the fake matches this code. It shares its on-disk layout with
`FsBlobStore` (`src/blob_layout.rs` — `ab/cdef…`) and is smoke-tested
against real Tigris in P11.
