//! The whole cycle on a scratch record: a check that fails naming what moved
//! and the command, a bless that writes it, and the same check passing.
//!
//! One test function, because it drives the process environment
//! (`LP_EMU_FIGURES_DIR`, `LP_EMU_BLESS`) and parallel tests would race on it.

use lp_emu_esp_figures::{BLESS_ENV, Figures, record_path};

#[test]
fn check_fails_bless_writes_check_passes() {
    let dir = std::env::temp_dir().join(format!("lp-emu-figures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: the only test in this binary, so nothing reads the environment
    // concurrently.
    unsafe {
        std::env::set_var("LP_EMU_FIGURES_DIR", &dir);
        std::env::remove_var(BLESS_ENV);
        std::env::remove_var("GITHUB_ACTIONS");
    }
    let path = record_path("chipx");
    std::fs::write(
        &path,
        "{\n  \"_about\": \"kept\",\n  \"boot.cycles\": 100,\n  \"boot.chain\": [\n    \"a\",\n    \
         \"main stack 45344 B\",\n    \"\"\n  ]\n}\n",
    )
    .unwrap();

    let observe = || {
        let mut f = Figures::new("chipx", "t::boot");
        f.int("boot.cycles", 116u64)
            .text("boot.chain", "a\nmain stack 45328 B\n")
            .string("boot.sha", "abc");
        f
    };

    // 1. A check: fails, naming all three, old → new, and the command.
    let err = std::panic::catch_unwind(|| observe().verify()).expect_err("moved figures fail");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        msg.contains("3 pinned firmware figures moved (chipx, t::boot)"),
        "{msg}"
    );
    assert!(msg.contains("boot.cycles: 100 → 116 (+16)"), "{msg}");
    assert!(
        msg.contains("line 2: \"main stack 45344 B\" → \"main stack 45328 B\""),
        "{msg}"
    );
    assert!(msg.contains("boot.sha: not recorded → \"abc\""), "{msg}");
    assert!(msg.contains("just bless-chips chipx"), "{msg}");

    // 2. A bless: rewrites the record in its one layout, prose kept.
    unsafe { std::env::set_var(BLESS_ENV, "1") };
    observe().verify();
    unsafe { std::env::remove_var(BLESS_ENV) };
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\n  \"_about\": \"kept\",\n  \"boot.chain\": [\n    \"a\",\n    \
         \"main stack 45328 B\",\n    \"\"\n  ],\n  \"boot.cycles\": 116,\n  \
         \"boot.sha\": \"abc\"\n}\n"
    );

    // 3. The same check passes, and a bless of unmoved figures writes nothing.
    observe().verify();
    let before = std::fs::metadata(&path).unwrap().modified().unwrap();
    unsafe { std::env::set_var(BLESS_ENV, "1") };
    observe().verify();
    unsafe { std::env::remove_var(BLESS_ENV) };
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        before
    );

    // 4. A positional figure: checked exactly, tagged in the failure, and a
    //    desk bless leaves CI's value alone.
    let positional = || {
        let mut f = Figures::new("chipx", "t::gap");
        f.positional_int("boot.gap", -64i64);
        f
    };
    std::fs::write(&path, "{\n  \"boot.gap\": -96\n}\n").unwrap();
    let err = std::panic::catch_unwind(|| positional().verify()).expect_err("a moved gap fails");
    let msg = err.downcast_ref::<String>().cloned().unwrap_or_default();
    assert!(
        msg.contains("boot.gap: -96 → -64 (+32)   [positional]"),
        "{msg}"
    );
    assert!(msg.contains("the record holds CI's value"), "{msg}");
    unsafe { std::env::set_var(BLESS_ENV, "1") };
    positional().verify();
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\n  \"boot.gap\": -96\n}\n",
        "a desk bless does not write a positional figure"
    );
    unsafe { std::env::set_var("GITHUB_ACTIONS", "true") };
    positional().verify();
    unsafe {
        std::env::remove_var(BLESS_ENV);
        std::env::remove_var("GITHUB_ACTIONS");
    }
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{\n  \"boot.gap\": -64\n}\n",
        "CI's bless does"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
