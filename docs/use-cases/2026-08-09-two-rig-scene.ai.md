# The two-rig scene — shared visuals, disjoint control

The multi-rig archetype: one project containing **several
physically independent rigs** that exist together for convenience
and share *content*, but not *wiring*.

The concrete case: dome + orb at Burning Man. A (dome-rig,
orb-rig, scene) project where the **visual product is shared**
between the rigs — one look across the camp — while the **control
products are not**: each rig has its own fixtures, outputs, and
cabling. The orb is wired once and never re-patched; the dome is
re-patched every build.

## Demands on the system

- Visual products flow project-wide; control products stay within
  their rig.
- Patching one rig must be impossible to confuse with patching the
  other; clearing the dome's patch must not threaten the orb's.
- Output identity must stay unambiguous project-wide even though
  the rigs are independent.

## The D1 proposal (for ratification)

This archetype is the reason the patching surface's scope cannot
be simply "the project" or "the module". Proposed resolution
(mapping & patching vision, D1):

> **Patch scope = the control domain**: the connected component of
> control-product flow (the fixtures and outputs reachable from
> one another over control buses). A dome+orb project has one
> shared visual space but **two disjoint patch domains**. The
> patching surface is project-wide as a *view*; control domains are
> **validation scopes** within it — overlap checking runs
> per-domain, and patch-mode presentation groups by domain.
> Output short codes remain unique **project-wide** (not
> per-domain), so wiring two domains together later can never
> collide identities.

Consequences if ratified: no per-domain surface or navigation is
needed (domains are overlays/filters in the one view); validation
and clear-patch operations take a fixture *selection*, which
naturally respects domain boundaries without modeling them as
containers.

## Example project

Future — a miniature two-rig project (two modules, each fixture +
output, one shared visual bus) becomes buildable the moment
multi-fixture outputs land, and is worth shipping once the
patching surface can *show* domains. Until then this document is
the paper fixture for D1.

Provenance: mapping & patching surface vision, 2026-08-09
(planning dir `2026-08-09-0048-mapping-patching-surface`).
