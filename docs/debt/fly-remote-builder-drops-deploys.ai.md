---
status: carried
since: 2026-10-29
logged: 2026-10-29
area: CI / deploy-cloud.yml (flyctl deploy --remote-only)
related:
  - .github/workflows/deploy-cloud.yml
---
# Fly's remote builder drops a deploy mid-build

**Shape** — "Deploy Cloud Service" builds the image on fly.io's remote
builder (`flyctl deploy --remote-only`), which streams the docker build's
status back over one RPC. When that stream drops, the step fails with
`Error: failed to fetch an image or build from source: error building:
failed to receive status: rpc error: code = Unavailable desc = error reading
from server: EOF`, partway through compiling `lp-cloud-server`. Nothing in
the tree caused it: the same commit deploys on a re-run. The workflow has no
retry, so every drop is a merge that never reaches lightplayer.app until
somebody notices.

**Carrying cost** — a merge's release firmware ships and its tag is Latest,
but `/healthz` stays on the previous main. A session shipping the merge waits
on a deploy that has already failed, reads the log, and re-runs it by hand.
Studio in the field runs the older build in the meantime.

**Workarounds** —

```bash
id=$(gh run list --commit <merge sha> --workflow "Deploy Cloud Service" --json databaseId -q '.[0].databaseId')
gh run view $id --log-failed | grep -E "^.*Error"      # the rpc EOF, not a compile error
gh run rerun $id --failed                            # same commit; it deploys
curl -fsS https://lightplayer.app/healthz            # build == the merge sha
```

If the log shows a real compile error rather than the RPC EOF, it is not
this entry.

**Incident log** —
- 2026-10-29 08:03 UTC — #1328's merge (`3d9baefa0`): RPC EOF at 121 s of the
  build; the re-run deployed.
- 2026-10-29 11:09 UTC — #1329's merge (`f7c5cd770`): the same EOF at 98 s;
  re-run.
- 2026-10-31 08:38 UTC — #1334's merge (`88ccd87ff`): the same EOF about
  120 s into the build, while compiling `lp-cloud-server`; re-run with
  `gh run rerun --failed`. The third hit in three days. Release firmware was
  green, so only the Studio bundle waited.

**Exit criteria** — a dropped builder stream no longer fails a deploy without
a person: the deploy step retries the remote build once (or falls back to a
local build and push) on that error, and a red "Deploy Cloud Service" means
the tree is broken.
