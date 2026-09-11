//! The ABI's portable half, held to account on any host.
//!
//! The numbers and the grammar are a contract with a hand-written JavaScript
//! file that cannot be type-checked against this crate. So they are asserted
//! literally here: a renumbered outcome or a renamed config key fails a
//! `cargo test` long before it fails a Worker, where the symptom would be a
//! board that boots and then means something else.

use super::*;
use lp_riscv_emu::mach::HartFault;

/// The refusal text for a config that must not parse. `Config` has no
/// `Debug` and wants none — it is a bag of the builder's own types — so the
/// "it did parse" case is a panic with the text that should have failed.
fn refusal(text: &str) -> String {
    match Config::parse(text) {
        Ok(_) => panic!("`{text}` must not parse"),
        Err(reason) => reason,
    }
}

#[test]
fn the_outcome_codes_are_the_numbers_the_worker_mirrors() {
    // Literal, not derived: these six numbers are the wire.
    assert_eq!(code_for(&Outcome::Deadline { cycle: 7 }), 0);
    assert_eq!(code_for(&Outcome::ExitMatched { cycle: 7 }), 1);
    assert_eq!(
        code_for(&Outcome::Fault {
            cycle: 7,
            pc: 0x4000_0000,
            fault: HartFault::TrapVectorFetch { vector: 0 },
        }),
        2
    );
    assert_eq!(
        code_for(&Outcome::Reset {
            cycle: 7,
            source: "USB_DEVICE chip_rst (serial)",
            strap: Strap::Download,
        }),
        4
    );
    assert_eq!(
        code_for(&Outcome::Breakpoint {
            cycle: 7,
            pc: 0x4000_0000
        }),
        5
    );
    assert_eq!(
        code_for(&Outcome::WallTimeout { cycle: 7 }),
        6,
        "unreachable through this ABI, and numbered anyway so a seventh \
         outcome cannot silently take a taken number"
    );
    assert_eq!(ABI_VERSION, 1);
}

#[test]
fn the_error_codes_are_the_numbers_the_worker_mirrors() {
    assert_eq!(AbiError::NoMachine.code(), -1);
    assert_eq!(AbiError::AlreadyCreated.code(), -2);
    assert_eq!(AbiError::BadConfig.code(), -3);
    assert_eq!(AbiError::BuildFailed.code(), -4);
    assert_eq!(AbiError::BufferTooSmall.code(), -5);
    assert_eq!(AbiError::BadBuffer.code(), -6);
    assert_eq!(AbiError::OutOfRange.code(), -7);
    assert_eq!(AbiError::Unsupported.code(), -8);
    // Every code is negative, and a count is not: that separation IS the
    // ABI's return convention.
    for code in [
        AbiError::NoMachine,
        AbiError::AlreadyCreated,
        AbiError::BadConfig,
        AbiError::BuildFailed,
        AbiError::BufferTooSmall,
        AbiError::BadBuffer,
        AbiError::OutOfRange,
        AbiError::Unsupported,
    ] {
        assert!(code.code() < 0, "{code:?}");
    }
}

#[test]
fn an_empty_config_is_the_board_the_tab_host_wants() {
    let cfg = Config::parse("").expect("an empty config is the default board");
    assert_eq!(
        cfg.boot,
        BootMode::RomUp,
        "a board boots itself out of flash"
    );
    assert_eq!(cfg.flash_len, crate::flash::DEFAULT_FLASH_LEN);
    assert_eq!(cfg.grade, TimeGrade::T1);
    assert!(!cfg.strict, "a board is not a bring-up run");
    assert!(
        cfg.reboot_on_reset,
        "a reset dance must reboot the chip, not end the world"
    );
    assert_eq!(
        cfg.usb_host,
        UsbHost::Absent,
        "the cable is the consumer's to plug in, with a control line"
    );
    assert_eq!(cfg.strap, Strap::App);
    assert_eq!(cfg.reset_cause, ResetCause::PowerOn);
    assert_eq!(cfg.mac, None);
}

#[test]
fn every_key_parses_and_comments_and_blank_lines_do_not() {
    let cfg = Config::parse(
        "\
# the walk's board
mac = 02:c6:7a:b0:00:01

boot=rom-up
grade=t2
flash_len=4194304
strict=1
reboot_on_reset=0
usb_host=attached   # a cable AND an open port, as the CLI means it
strap=download
reset_cause=usb-uart-hpsys
",
    )
    .expect("every key above is a key");
    assert_eq!(cfg.mac, Some([0x02, 0xc6, 0x7a, 0xb0, 0x00, 0x01]));
    assert_eq!(cfg.boot, BootMode::RomUp);
    assert_eq!(cfg.grade, TimeGrade::T2);
    assert_eq!(cfg.flash_len, 4 * 1024 * 1024);
    assert!(cfg.strict);
    assert!(!cfg.reboot_on_reset);
    assert_eq!(cfg.usb_host, UsbHost::Attached { draining: true });
    assert_eq!(cfg.strap, Strap::Download);
    assert_eq!(cfg.reset_cause, ResetCause::UsbUartHpSys);

    // The machine's own three words, and not a second spelling of them:
    // `attached` is the cable in with the port OPEN, `attached-idle` is the
    // cable in with it closed. Asserted here because inverting the pair is
    // silent — the board builds either way and then says nothing.
    assert_eq!(
        Config::parse("usb_host=attached-idle").unwrap().usb_host,
        UsbHost::Attached { draining: false },
        "a cable is not a port open"
    );
    assert_eq!(
        Config::parse("usb_host=absent").unwrap().usb_host,
        UsbHost::Absent
    );
    assert_eq!(Config::parse("boot=direct").unwrap().boot, BootMode::Direct);
}

/// A host that misspells a key must be told, not handed a default board.
#[test]
fn a_key_nobody_knows_is_refused_by_name() {
    let err = refusal("mac=02:c6:7a:b0:00:01\nusbhost=attached\n");
    assert!(err.contains("line 2"), "{err}");
    assert!(err.contains("no config key `usbhost`"), "{err}");
    assert!(
        err.contains("usb_host"),
        "the refusal lists the keys there are: {err}"
    );
}

#[test]
fn a_value_that_does_not_parse_names_its_line_and_what_was_wanted() {
    for (text, wanted) in [
        ("boot=sideways", "a boot mode"),
        ("usb_host=maybe", "a host state"),
        ("strap=sideways", "a strap"),
        ("reset_cause=cosmic-ray", "a reset cause"),
        ("strict=yes-please", "0 or 1"),
        ("flash_len=four-megs", "a byte count"),
        ("mac=02:c6", "a MAC address"),
    ] {
        let err = refusal(text);
        assert!(err.starts_with("line 1: "), "{text}: {err}");
        assert!(err.contains(wanted), "{text}: {err}");
    }
    let err = refusal("grade=t9");
    assert!(err.starts_with("line 1: "), "{err}");

    let err = refusal("just a word");
    assert!(err.contains("is not key=value"), "{err}");
}

/// The config really does build the machine it describes — the parser and
/// the builder agreeing is the claim, not the parser alone.
#[test]
fn the_config_builds_the_machine_it_describes() {
    let cfg = Config::parse("mac=02:c6:7a:b0:00:01\ngrade=t2\nflash_len=65536\n").unwrap();
    let machine = cfg
        .builder(FlashBacking::Bytes(Vec::new()), AppSource::None)
        .build()
        .expect("a blank rom-up board builds");
    assert_eq!(machine.efuse().mac, [0x02, 0xc6, 0x7a, 0xb0, 0x00, 0x01]);
    assert_eq!(machine.time_grade(), TimeGrade::T2);
    assert_eq!(machine.flash().lock().unwrap().len(), 65536);
    assert_eq!(machine.boot_mode(), BootMode::RomUp);
    assert!(
        !machine.has_image_at_reset_vector(),
        "an empty byte slice is a blank chip"
    );
    assert!(
        machine.usb_sj_host_handle().is_some(),
        "every board the ABI builds has the live source the host writes into"
    );
}

/// The length rule reaches the host as a `BuildFailed` with a sentence,
/// which is the difference between a refusal and a number.
#[test]
fn an_image_longer_than_the_chip_is_refused_by_the_builder() {
    let cfg = Config::parse("flash_len=4096").unwrap();
    let err = match cfg
        .builder(FlashBacking::Bytes(vec![0xe9; 8192]), AppSource::None)
        .build()
    {
        Ok(_) => panic!("8 KiB must not fit a 4 KiB chip"),
        Err(e) => e.to_string(),
    };
    assert!(err.contains("larger than the 4096-byte chip"), "{err}");
}
