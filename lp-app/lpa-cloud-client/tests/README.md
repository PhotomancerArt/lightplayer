# Scenario tests

`scenarios.rs` holds the flagship stories — one test function per story,
named for the story, meant to be read top to bottom. `builders/mod.rs` holds
the vocabulary they are written in.

Everything runs against one `InProcessServer`: the real domain over the real
in-memory adapters, with two or three clients talking to it. No network, no
mock, no sleeps, no wall clock.

## The nouns

| | what it is |
| --- | --- |
| `TestWorld::new()` | the world: one service, and the counters that name everything in it |
| `td.user()` | somebody with an account, signed in — `user-1`, then `user-2` |
| `td.visitor()` | somebody who followed a link without signing in |
| `td.invitee()` | an email address with no account behind it yet |
| `user.project()` | a new local project with one saved version, not on the cloud |
| `user.open_shared(&link)` | that person's tracking copy of a shared project |
| `user.open_url("https://…/p/slug-prj…")` | the same thing, opened the way a person does |

A `Project` is **one copy of one project on one machine**. Two handles on the
same uid — the owner's and a member's — are two copies, which is what makes
the two-client stories say what they mean.

## The verbs

Local: `edit(note)` (writes the shader and saves), `fork()`.

Cloud: `publish(access)`, `set_access(a)`, `add_member(email)`,
`collaborator()` (`add_member` plus the login that resolves it), `push()`,
`pull()`, `fast_forward(&pulled)`, `resolve(&pulled, side)`.

Every one of these goes through the crate's real operation. Nothing writes a
store directly, so no scenario can assert about a state the application could
not reach.

Refusals have their own verbs, so a scenario never unwraps a `Result`:
`push_error()`, `pull_error()`, `open_shared_error(&link)` each return the
`SyncError` and fail the test if the operation *succeeded*.

## The readers

Handles re-read; they never hand back something they cached.

- `shader()` — what the working copy says now
- `shader_at(version)` — what a banked version says, including a set-aside one
- `head()` — the local history's current version
- `relation_to(version)` — how this copy's history sees a version
- `bound_to()` — the cloud project this copy tracks, if any
- `server_heads()` — the service's frontier, asked for again
- `uid()`

So an assertion is a question put to a handle:

```rust
assert_eq!(tracker.head(), dome.head());
assert_eq!(dome.server_heads(), vec![dome.head()]);
```

and not a comparison against a hash captured earlier. The exception is the
typed reports (`PullReport`, `ClobberReport`, …) — those are the operation's
own answer, and `resolved.set_aside` is the only way to name the version a
clobber put down.

## Determinism

Names come from counters (`user-1`, `proj-2`); a project's uid is minted from
its counter. Time comes from the service's clock, which advances one tick per
recorded event — there is no wall clock and nothing sleeps. Two runs produce
identical uids, identical timestamps, and identical content hashes.

## Adding a scenario

Write the story with the vocabulary above. If the test needs a comment to be
followed, the fix is a better builder verb, not the comment.

Fixtures are never shared between tests: each one starts at `TestWorld::new()` and
builds exactly the world its story needs.
