# M2 Playlist Node Summary

## What was built

- Added `Playlist` model support with `PlaylistDef`, `PlaylistEntry`, `PlaylistState`, slot views, TOML parsing, and `NodeDef`/`NodeKind` integration.
- Added recursive project loading for playlist entry child nodes, including path-backed and inline `NodeInvocation` children.
- Added entry-local trigger bindings: `[entries.N.bindings.trigger] source = "bus#trigger"` registers against `entries[N].trigger` on the playlist node.
- Added default visual output bindings for top-level visual nodes, while suppressing those defaults for playlist entry children.
- Added resolver/render plumbing so a playlist can publish `entry_time` before resolving children and can delegate texture rendering or direct sampling to child visual products.
- Added `PlaylistNode` runtime selection, duration handling, repeated entry-trigger restart, `entry_time`, and outgoing `fade_after` crossfade support.
- Added `examples/button-playlist` with `idle` and `active` shaders, a D9 button trigger, fixture/output wiring, and real example render coverage.

## Decisions for future reference

#### Entry Triggers

- **Decision:** Triggers live on playlist entries, not on the playlist as a generic "next" input.
- **Why:** A button press can explicitly start or restart `active` without cycling through idle or depending on global playlist state.
- **Rejected alternatives:** A single playlist-level trigger that advances to the next entry.
- **Revisit when:** We add separate generic next/previous playlist controls.

#### Playlist Time

- **Decision:** Expose one public time slot, `entry_time`, and keep absolute `switch_time` internal.
- **Why:** Child shaders need "time since this entry activated"; global time remains available through ordinary bindings.
- **Rejected alternatives:** A separate child clock node, multiple public time slots, or mutating the global clock bus.

#### Child Output Binding

- **Decision:** Playlist entry children do not receive the default `output -> bus#visual.out` fallback.
- **Why:** Their output is owned by the playlist; letting them also publish to the bus would bypass playlist selection and crossfade.
- **Rejected alternatives:** Require every entry child to explicitly opt out of default visual output.

#### Crossfade

- **Decision:** `fade_after` belongs to the outgoing entry, with `default_fade` as the fallback.
- **Why:** It reads as "how this entry leaves" and lets idle/active use different fade behavior without extra transition objects.
- **Rejected alternatives:** A fade-before field on the incoming entry for this first slice.

#### Same-Frame Publication

- **Decision:** `PlaylistNode` publishes produced slots into the current resolver session before resolving children.
- **Why:** A child shader can bind `time <- ..#entry_time` without recursively ticking the playlist.
- **Rejected alternatives:** A separate clock node for entry time or resolver recursion special cases tied only to playlists.
