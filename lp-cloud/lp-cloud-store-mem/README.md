# lp-cloud-store-mem

The fake is the dev backend.

In-memory adapters for every `lp-cloud-domain` port: `MemMetaStore`,
`MemBlobStore`, `MemClock`, `MemIdMint`. Run the whole cloud service against
these and you get a working service with no database, no object store, and
no wall clock — which is what `LP_CLOUD_STORE=mem` serves locally (D13) and
what every test above this layer runs on.

This is **not a stub**. Every port method is implemented for real, the
domain's own test suite runs against it, and P04 holds it and the SQLite
adapter to one shared conformance suite. A fake that drifts from the real
store is a slow-motion data corruption bug: the tests keep passing while
production stops agreeing with them.

## Why `BTreeMap` everywhere

Iteration order is key order, so a project listing, a membership list, and a
head frontier come out the same on every run. A test that asserts on order
is then asserting on the store's contract rather than on a hash seed.

## The two that are not stores

`MemClock` stands still until something moves it — `advance(&self, …)` takes
`&self` so a test can expire a session while the service still owns the
clock. `MemIdMint` is a counter, not an rng: every draw differs and the same
program run twice draws the same bytes, which is what makes a minted `usr_`
uid assertable.

**`MemIdMint` must never back a real deployment.** Session tokens are bearer
credentials; a counting one is a login bypass. The server edge injects a
cryptographically-secure mint.
