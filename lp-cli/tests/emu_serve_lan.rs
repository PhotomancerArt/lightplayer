//! Wi-Fi plan P12 §3 — `lp-cli emu serve`'s virtual LANs, the half that
//! needs no firmware: `--lan <name>=<fixture>`, a board's `lan=<name>`, each
//! board's forward in `GET /boards`, the door's `renumber` verb, and the LAN
//! browse.
//!
//! **Not `#[ignore]`d**: the boards are `kind=rom-up` with nothing on their
//! chips, so a bare `cargo test -p lp-cli` runs the whole file. A board joins
//! its LAN at build (its lease reserved, its forward bound) whether or not
//! the guest ever runs an app, which is what makes the host's side testable
//! with no image. What a joined board says over its forward is
//! `emu_lan_link.rs` (one board, `emu run --lan`) and the walk
//! (`scripts/emu/walk-wifi-emu-lan.mjs`, two boards on one served LAN).
//!
//! Nothing here asserts a cycle or a duration — a served LAN runs on the
//! host's clock and a socket is not deterministic. The wall nets in
//! `support` are a safety net, never an input.

mod support;

use support::{Serve, scratch};

/// Test values only, never a real network's.
const FIXTURE: &str = r#"# emu_serve_lan.rs: made-up test values only.
[[access_point]]
name = "lp-walk-net"
password = "correct-horse-42"
signal_dbm = -50
"#;

/// Two boards on one LAN, one on none: the two on it each have a forward of
/// their own, and the third has none of the LAN's fields.
#[test]
fn boards_naming_one_lan_share_it_each_with_its_own_forward() {
    let serve = lan_serve();

    let mut forwards = Vec::new();
    for id in ["c6-a", "c6-b"] {
        let board = serve.board(id);
        assert_eq!(board["lan"], "home", "{board:#}");
        let forward = board["forward"].as_str().expect("a forward").to_string();
        let port = forward
            .strip_prefix("lan:127.0.0.1:")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or_else(|| {
                panic!("`lan:127.0.0.1:<port>`, as `lp-cli … lan:` takes it: {forward}")
            });
        assert_ne!(port, serve.port(), "the forward is a door of its own");
        assert!(
            board["address"].is_null(),
            "a blank chip never runs DHCP: {board:#}"
        );
        // The configuration field stays beside the LAN's: nothing engaged
        // on a chip with no app.
        assert_eq!(board["configuration"], "lp-emu:esp32c6:t1", "{board:#}");
        forwards.push(forward);
    }
    assert_ne!(forwards[0], forwards[1], "two boards, two forwards");

    let alone = serve.board("c6-c");
    for key in ["lan", "forward", "address"] {
        assert!(
            alone[key].is_null(),
            "{key} on a board on no LAN: {alone:#}"
        );
    }
}

/// `renumber` is the door's own verb, answered in its place in the reply
/// order; the machine's verbs around it are answered by the machine.
#[test]
fn renumber_is_the_doors_verb_and_waits_its_turn() {
    let serve = lan_serve();
    let mut control = serve.control("c6-a");
    let state = control.cmd("state");
    assert!(state.starts_with("ok state "), "the machine's own: {state}");
    let renumbered = control.cmd("renumber");
    assert!(
        renumbered.starts_with("ok renumber lan=home board=c6-a"),
        "{renumbered}"
    );
    assert!(
        control.cmd("renumber now").starts_with("err renumber:"),
        "takes no arguments"
    );
    assert!(control.cmd("state").starts_with("ok state "));

    let mut alone = serve.control("c6-c");
    let refused = alone.cmd("renumber");
    assert!(
        refused.starts_with("err renumber: board `c6-c` is on no served LAN"),
        "{refused}"
    );
}

/// The browse answers with what the boards said, here nothing (no board on
/// the LAN has an app), once its wait is up; an unknown LAN is a 404 and a
/// bad query a 400.
#[test]
fn the_lan_browse_answers_as_json_and_names_its_lan() {
    let serve = lan_serve();
    let body = serve.get("/lans/home/browse?service=_lightplayer._tcp.local&wait_ms=300");
    let json: serde_json::Value = serde_json::from_str(&body).expect("the browse is JSON");
    assert_eq!(json["lan"], "home", "{json:#}");
    assert_eq!(json["service"], "_lightplayer._tcp.local");
    assert_eq!(
        json["instances"].as_array().map(Vec::len),
        Some(0),
        "no app, no answer: {json:#}"
    );
    assert!(json["answers"].is_array(), "{json:#}");

    assert_eq!(serve.get_status("/lans/nowhere/browse"), 404);
    assert_eq!(serve.get_status("/lans/home/browse?colour=blue"), 400);
}

/// A board naming a LAN nobody declared is refused before anything starts.
#[test]
fn a_lan_nobody_declared_is_refused() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lp-cli"))
        .args([
            "emu",
            "serve",
            "--listen",
            "127.0.0.1:0",
            "--board",
            "c6-a=blank,kind=rom-up,lan=home",
        ])
        .output()
        .expect("running lp-cli emu serve");
    assert!(!output.status.success(), "it must not serve");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--lan home="), "{stderr}");
}

/// `c6-a` and `c6-b` on LAN `home`, `c6-c` on none; all three blank.
fn lan_serve() -> Serve {
    let dir = scratch();
    std::fs::create_dir_all(&dir).expect("the scratch dir");
    let fixture = dir.join("lan.toml");
    std::fs::write(&fixture, FIXTURE).expect("the fixture");
    let lan = format!("home={}", fixture.display());
    Serve::start_specs(
        &[
            "c6-a=blank,kind=rom-up,lan=home".to_string(),
            "c6-b=blank,kind=rom-up,lan=home".to_string(),
            "c6-c=blank,kind=rom-up".to_string(),
        ],
        &["--lan", &lan],
        dir,
    )
}
