---
status: open
found: 2026-10-10      # how: e2e (lpa-studio-core connected tests, over lpa-link's fake device)
area: lpa-link providers/fake_device (FakeDeviceCore::loaded_projects, the synthetic heartbeat)
class: fidelity
related:
  - lp-app/lpa-studio-core/src/app/studio/studio_device_e2e_tests/connected_tests.rs (the play-tier connect test's workaround)
  - lp2025/2026-10-08-2330-connected-in-the-card (P4, where it was found; filed by P8 as the director ruled)
---
# The fake board's heartbeat misses a project it loaded at boot

**Symptom** — a fake board scripted to resume its project at boot
(`FakeLightPlayerState::with_loaded_project`, the shape real firmware has
had since the startup resume) answers `ListLoadedProjects` with the
project from its first request, but its heartbeat says
`"loaded_projects":[]` until something asks. Studio reads the heartbeat,
so the board's card says it runs nothing: Connect is disabled, "Nothing on
it yet", on a board that is running a project. The connected plan's
play-tier test (`connected_tests.rs`, the test that connects with the play
password) works around it: it opens the board once by its address so the
fake learns what it runs, presses Done, and only then presses Connect.

**Root cause** — the fake's heartbeat is synthetic. It has no server
registry to read, so `FakeDeviceCore::note_server_frame` builds
`loaded_projects` from the answers the server gave on the wire (a
`ListLoadedProjects` snapshot, a `LoadProject` reply paired with its
request's path) and from nothing else, deliberately: a load that failed
must not make the fake claim more than hardware would. A boot-time load is
not an answer to any request, so it never reaches that list. Real firmware
(`fw-esp32c6` `boot::auto_load_project`) assembles every heartbeat from the
server's own registry, so the resumed project is in its first heartbeat.
The fake models the wire's evidence where the firmware reports its state.

**Fix** — none yet. The likely shape: when the script loads the seeded
project at boot, seed `loaded_projects` from the server's own answer to an
internal `ListLoadedProjects` (or the boot load's handle and path) before
the first heartbeat, keeping the rule that nothing is inferred from a
request alone. Then drop the open-and-Done step from the play-tier test.

**Regression coverage** — none yet. The fix would want a fake-device test
that a `with_loaded_project` board's first heartbeat names the project.

**Lesson** — a test double that reports what it has *seen* on the wire,
where the real thing reports what it *is*, diverges at every state the real
thing reaches without a request: boot, a startup resume, a reset. A core
test written against it then needs a priming step a real board never
needs, and the step hides the gap.
