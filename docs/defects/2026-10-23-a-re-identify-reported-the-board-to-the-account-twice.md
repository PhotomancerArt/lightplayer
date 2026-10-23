---
status: fixed
found: 2026-10-23      # how: CI red on main (Validate x64, job 120275282279, at bd0ff431c)
fixed: 2026-10-23      # lpa-devices: the late-hello re-identify settles on the hello in hand
area: lp-app/lpa-devices identify (the late-hello re-identify) × lpa-studio-core's once-per-connection effects (LoginWindow)
class: state-conflation
related:
  - docs/defects/2026-10-22-a-hello-after-identify-settled-never-asks-what-the-board-runs.md
  - lp2025/2026-10-20-2335-s3-radio-and-ota (M7 P12a)
---
# A re-identify reported the board to the account twice

**Symptom** — `main` went red at bd0ff431c in `Validate (x64)`.
`account_board_list_tests::a_usb_connect_that_finds_the_accounts_key_reports_the_board_once`
saw two `Report`s for one USB connection where it expects one:
`once per connection: [Report(… seen_on: Usb), Report(…)]`, left 2, right
1. It was intermittent: `main` was green on the commits right after the
change that caused it.

**Cause** — #1265's fix for a slow board
(`2026-10-22-a-hello-after-identify-settled-never-asks-what-the-board-runs.md`).
When an identify settled with no hello and the board's hello came later, it
spawned a plain identify. A plain identify **asks** the board for a hello, and
the answer moved the window's `hello_at` (`Evidence::hello_heard_at`) a second
time on the same connection.

`lpa-studio-core` takes "one connection" to be `LoginWindow { link, hello_at }`,
so the answer looked like a new connection to everything keyed on it:
- the access list read (`AccessController::synced`);
- the account's board report (`sighted`: the failing test);
- the login (`AccessSession::logged_in`);
- the restart watch (`restarts`, which reads a newer hello as "restarted");
- the network panel's window (`network_controller`);
- the card's picture feed (`FeedTarget::hello_at`, which restarts on a new one).

The bench only takes the late-hello path when the fake board's hello lands
after identify's verdict, which a loaded CI runner can make happen. That is
why it was intermittent.

**Fix** — the late-hello re-identify is
`IdentifyActivity::on_hello_in_hand` (`Device::spawn_identify_on_hello_in_hand`).
It asks the board for no hello, settles on the one already in the window at
its first tick, and asks `ListLoadedProjects` as any LightPlayer verdict does.
The window keeps the board's own hello, so none of the effects above runs
twice on one connection. Every other identify asks exactly as before.

An answered hello still moves `hello_at` everywhere else. The update's
between-legs check relies on that after a reset that kept the port open
(`a_hello_older_than_the_leg_never_starts_the_next_one`), which is why the fix
is in the re-identify and not in how the fold reads hellos.

**Test** — `a_hello_after_identify_settled_unrecognized_reidentifies_and_asks_what_it_runs`
(`lpa-devices/src/device.rs`) drives the late hello explicitly: boot lines
only, the 5 s verdict, then the board's hello 180 s later. It now asserts:
- the re-identify sends no hello;
- `hello_heard_at` stays the board's own;
- the re-identify settles at its first tick as a LightPlayer and asks what the board runs.

The account test no longer depends on when the bench's fake board answers:
whichever path identify takes, one connection has one `hello_at`.
