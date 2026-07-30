//! M6 stretch: link + run the two-object reloc fixtures (assembled by
//! `fixtures/reloc/build.sh` with the esp toolchain's GNU as — instruction
//! bytes are never hand-written) and check them against host-side oracles.
//! Each pair is also linked by GNU ld (`obj/<name>.ld.elf`) and run through
//! the plain linked-executable loader as a behavioral differential: the
//! prototype linker and GNU ld must reach the same result (the images are not
//! expected to be byte-identical — GNU ld relaxes, we do not).
//!
//! Tests SKIP with a note when the fixtures have not been built, mirroring
//! `tests/fixtures.rs`, so the stable workspace never needs the esp toolchain.

#![cfg(feature = "reloc")]

use lp_xt_elf::reloc::{LinkError, link_objects, run_linked};
use lp_xt_elf::run_elf;
use lp_xt_emu::RunOutcome;
use std::path::PathBuf;

fn reloc_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fixtures/reloc/obj")
}

/// Load `<name>_main.o` + `<name>_lib.o`, or `None` (skip) if unbuilt.
fn load_pair(name: &str) -> Option<(Vec<u8>, Vec<u8>)> {
    let dir = reloc_dir();
    let main = std::fs::read(dir.join(format!("{name}_main.o")));
    let lib = std::fs::read(dir.join(format!("{name}_lib.o")));
    match (main, lib) {
        (Ok(m), Ok(l)) => Some((m, l)),
        _ => {
            eprintln!(
                "SKIP {name}: fixtures/reloc/obj/{name}_*.o not found — run \
                 fixtures/reloc/build.sh (esp toolchain) first"
            );
            None
        }
    }
}

/// Link the pair, run `lp_main(arg)`, and assert the result; then run the
/// GNU-ld oracle executable with the same arg and assert both agree.
#[track_caller]
fn assert_pair(name: &str, arg: u32, expected: u32) {
    let Some((main, lib)) = load_pair(name) else {
        return;
    };
    let run = run_linked(&[&main, &lib], "lp_main", arg).expect("link+run");
    assert_eq!(run.outcome, RunOutcome::Ok(expected), "{name}({arg})");
    assert_eq!(run.panic, None, "{name}({arg}) panicked");

    let oracle_path = reloc_dir().join(format!("{name}.ld.elf"));
    match std::fs::read(&oracle_path) {
        Ok(elf) => {
            let oracle = run_elf(&elf, arg).expect("run GNU-ld oracle");
            assert_eq!(
                oracle.outcome, run.outcome,
                "{name}({arg}): GNU-ld oracle disagrees with prototype linker"
            );
        }
        Err(_) => eprintln!("SKIP {name} oracle: {} not found", oracle_path.display()),
    }
}

/// Cross-object call8: lp_main(a) = builtin_mix(a, 7) + 1 = 2a + 8.
#[test]
fn mix() {
    assert_pair("mix", 10, 28);
    assert_pair("mix", 0, 8);
    assert_pair("mix", 1000, 2008);
}

/// Function-pointer literal (R_XTENSA_32 to a function), cross-object data +
/// bss literals, callx8 + call8: lp_main(i) = 2*table[i] + 2.
#[test]
fn funptr() {
    let table = [10u32, 20, 30, 40];
    for (i, &v) in table.iter().enumerate() {
        assert_pair("funptr", i as u32, 2 * v + 2);
    }
}

/// Cross-object calls in both directions: lp_main(x) = (x + 0x123) & 0xff.
#[test]
fn pingpong() {
    for x in [0u32, 0xF0, 0xFFFF_FF00] {
        assert_pair("pingpong", x, x.wrapping_add(0x123) & 0xff);
    }
}

/// The linked image resolves symbols at the documented addresses: text (and
/// literals) in the I-bus half, data/bss in the D-bus half.
#[test]
fn layout_regions() {
    let Some((main, lib)) = load_pair("funptr") else {
        return;
    };
    let image = link_objects(&[&main, &lib]).expect("link");
    let lp_main = image.symbol("lp_main").expect("lp_main");
    let table = image.symbol("table").expect("table");
    let counter = image.symbol("counter").expect("counter");
    use lp_xt_elf::reloc::{DATA_BASE, TEXT_BASE};
    assert!(
        (TEXT_BASE..TEXT_BASE + 0x1_0000).contains(&lp_main),
        "{lp_main:#x}"
    );
    assert_eq!(lp_main % 4, 0, "call8 entry must be 4-aligned");
    assert!(
        (DATA_BASE..DATA_BASE + 0x1_0000).contains(&table),
        "{table:#x}"
    );
    assert!(counter > table, "bss follows data in the same region");
}

/// A missing definition surfaces as UndefinedSymbol, naming the symbol.
#[test]
fn undefined_symbol() {
    let Some((main, _lib)) = load_pair("mix") else {
        return;
    };
    let err = link_objects(&[&main]).unwrap_err();
    assert_eq!(
        err,
        LinkError::UndefinedSymbol {
            name: "builtin_mix".to_string()
        }
    );
}

/// Linking the same object twice is a duplicate global definition.
#[test]
fn duplicate_symbol() {
    let Some((_main, lib)) = load_pair("mix") else {
        return;
    };
    let err = link_objects(&[&lib, &lib]).unwrap_err();
    assert!(
        matches!(err, LinkError::DuplicateSymbol { ref name } if name == "builtin_mix"),
        "got {err:?}"
    );
}

/// A linked executable is not a relocatable input.
#[test]
fn rejects_executable_input() {
    let path = reloc_dir().join("mix.ld.elf");
    let Ok(elf) = std::fs::read(&path) else {
        eprintln!("SKIP rejects_executable_input: oracle not built");
        return;
    };
    let err = link_objects(&[&elf]).unwrap_err();
    assert!(
        matches!(
            err,
            LinkError::NotXtensaRelocatable {
                object_index: 0,
                ..
            }
        ),
        "got {err:?}"
    );
}
