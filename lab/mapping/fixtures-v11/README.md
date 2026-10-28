# Format-11 copies the lab reads

The mapping lab (`lab-mapping-files`, the lab page, and Studio's Graph,
which is built on it) reads project format 11 with map2d formats 1–5 and
patch files. Project format 12 (mapping rebuild M3) moved every project in
the repo to map2d format 6, which the lab does not read; it refuses a
format-12 project by name.

So the lab's baked-in projects, its tests and the Graph's stories read
these copies, taken whole from commit `8d42bdf3e` (the last commit before
the repo's projects were migrated), minus their READMEs and pictures:

| Copy | From |
|---|---|
| `playful-choker` | `catalog/projects/playful-choker` |
| `zook-dome` | `catalog/projects/zook-dome` |
| `fiber-headband` | `catalog/projects/fiber-headband` |
| `plasma` | `catalog/patterns/plasma` |
| `radiance-dome` | `lab/mapping/projects/radiance-dome` |
| `eaves` | `lab/mapping/projects/eaves` |
| `eaves-wired` | `lab/mapping/projects/eaves-wired` |

They are frozen: never edit or migrate them. The lab is retired at M4,
when Studio's new Graph edits format 6, and this directory goes with it.
