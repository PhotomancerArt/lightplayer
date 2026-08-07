# The cloud service has no abuse or quota posture

**Condition.** lp-cloud accepts authenticated blob/tree uploads (64 MiB
body cap per request is the only limit) and serves link-visible content
anonymously. There are no per-user quotas, no rate limits, no upload
size accounting, no DMCA/takedown mechanism, and Tigris bucket growth
is unbounded.

**Why it stands.** Ruled at planning (vision Q3/Q13): fine at crew
scale — the service's users are personally known. Building quota
machinery before there is anyone to abuse it is waste.

**Trigger to fix.** BEFORE any public announcement of the share
feature, or the first unknown-to-us registered user. Cheap first steps
when triggered: per-user byte accounting in the metastore (the blob
index already records sizes), a per-user cap, and fly's built-in
rate-limit knobs.

**Incident log.** (none yet)
