# The cloud service has no abuse or quota posture

**Condition.** lp-cloud accepts blob/tree uploads (64 MiB body cap per
request is the only limit) and serves link-visible content anonymously.
**Sharpened 2026-08-08 (API v3, sharing slice 1):** uploads no longer
require a session at all — an `Access::Edit` link-holder pushes
anonymously, and `PUT /b/{hash}` / `PUT /t/{hash}` answer any caller
(hash-verified, idempotent, but unmetered). Anonymous pushes also
append to project event logs. There are no per-user or per-IP quotas,
no rate limits, no upload size accounting, no DMCA/takedown mechanism,
and Tigris bucket growth is unbounded. Per-user byte accounting can no
longer cover the whole surface — anonymous writes need per-IP or
per-project accounting too.

**Why it stands.** Ruled at planning (vision Q3/Q13): fine at crew
scale — the service's users are personally known. Building quota
machinery before there is anyone to abuse it is waste.

**Trigger to fix.** BEFORE any public announcement of the share
feature, before habitually handing out `edit` links beyond personally
known testers, or the first unknown-to-us registered user. Cheap first steps
when triggered: per-user byte accounting in the metastore (the blob
index already records sizes), a per-user cap, and fly's built-in
rate-limit knobs.

**Incident log.** (none yet)
