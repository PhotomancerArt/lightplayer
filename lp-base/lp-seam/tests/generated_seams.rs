//! The generators, used from outside the crate the way firmware uses them.
//!
//! On the host the call shims run the silicon bodies directly (there is no
//! seam function off `riscv32`), so these prove the macros expand, check
//! their signatures and keep the silicon answer. That the riscv32 expansion
//! survives a real release build is proved on the C6 harness image
//! (`test_seam_abi`) under the emulator.

mod test_echo {
    lp_seam::seam_fn! {
        test_echo => fn(a: u32, b: u32, c: u32) -> u32 { a ^ b ^ c }
    }
}

mod test_take {
    lp_seam::seam_fn! {
        test_take => fn(endpoint: u32, buf: *mut u8, cap: u32) -> u32 {
            let _ = (endpoint, buf, cap);
            0
        }
    }
    lp_seam::engaged_byte!(test_take);
}

mod ws281x_wait_step {
    lp_seam::seam_fn! {
        ws281x_wait_step => fn() {}
    }
}

#[test]
fn a_value_seam_returns_its_silicon_answer_off_target() {
    assert_eq!(test_echo::call(1, 2, 4), 7);
    assert_eq!(test_echo::DECL.id, lp_seam::test_echo::ID);
}

#[test]
fn an_engaged_byte_reads_zero_in_the_image() {
    assert!(!test_take::engaged());
    assert_eq!(test_take::ENGAGED_BYTE, 0);
    assert_eq!(
        test_take::ENGAGED_ADDRESS.0,
        &test_take::ENGAGED_BYTE as *const u8 as *const ()
    );
    let mut buf = [0u8; 4];
    assert_eq!(test_take::call(0, buf.as_mut_ptr(), 4), 0);
}

#[test]
fn a_seam_with_no_arguments_or_result_still_calls() {
    ws281x_wait_step::call();
    assert_eq!(ws281x_wait_step::DECL.symbol, "lp_seam_ws281x_wait_step");
}
