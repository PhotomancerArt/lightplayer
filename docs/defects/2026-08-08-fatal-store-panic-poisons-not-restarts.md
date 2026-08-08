# A fatal store panic poisons the lock instead of restarting the node

**Mechanism.** The sqlite adapter's fail-fast policy (`store_fatal.rs`)
was written as `panic!`, and its recovery story — "the process
supervisor restarts us onto a Litestream restore" — assumed a panic
kills the process. It doesn't: the panic unwinds into
`spawn_blocking`, `with_service` resumes it onto the handler thread,
the request's connection drops (fly answers 502), and the store mutex
is **poisoned**. Every later request then panics on the poison — a
permanent zombie — while `/healthz`, which never takes the store lock,
keeps answering ok, so fly's health checks never restart the machine.
The design named a supervisor as its enforcement surface and never
invoked it.

**How it fired (2026-08-08).** The uid-format rework (#384, `prj_…` →
single-token base-32) owed a prod store wipe on next deploy — the
deployed binary's `PrefixedUid` parser refuses old-spelling uids, and
the wipe was the ratified alternative to a data migration. The #388
deploy went out, Litestream restored the OLD database, and the first
row decode (`usr_qY7LqRfrX26CGe8m`-era spelling) hit
`fatal("decoding a uid column", …)` → poisoned lock → every
`/p/<uid>` page and every API call answered 502 behind a green
healthz. Local dev never sees this (mem store, fresh files), and the
migration tests cover *schema* shape, not *data* spelling.

**Fix (this change).** `fatal()` now logs and `std::process::abort()`s
— the supervisor actually runs, and boot lands on a Litestream
restore. `with_service` treats a poisoned lock (any other panic that
unwound while holding the guard) the same way. If the data itself is
poison (as here), the node crashloops instead of zombieing — honest,
visible, and fly backs off; the remedy for THIS instance is the owed
store wipe, after which the client sync engine self-heals (sign-in
sweep republishes unbound projects; a bound push answering NotFound
falls back to re-publish).

**Residual risk / watch for.** Any future format change that touches
persisted cloud data must either ship a data migration or schedule the
wipe as part of the deploy, not as a memory note. The version-and-
refuse posture covers the wire; the *store* has no version gate on row
contents — rows are trusted to decode. If that ever changes, the gate
belongs in the migration chain, not in per-request decode.
