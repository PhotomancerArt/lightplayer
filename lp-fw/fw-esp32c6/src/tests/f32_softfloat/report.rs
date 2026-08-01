//! Pass/fail tally for the f32 soft-float harness.
//!
//! Prints one line per case as it runs — a hung or rebooting board still leaves
//! evidence of how far it got — and a single machine-greppable summary at the
//! end.

use esp_println::println;

#[derive(Default)]
pub struct Report {
    pub passed: u32,
    pub failed: u32,
}

impl Report {
    /// Record one case. `got`/`want` are printed as raw words because that is
    /// the only representation that survives the question being asked: a
    /// decimal rendering of an f32 goes through the very routines under test.
    pub fn record(&mut self, what: &str, ok: bool, got: u32, want: u32) {
        if ok {
            self.passed += 1;
            println!("[f32-soft] PASS {what}");
        } else {
            self.failed += 1;
            println!("[f32-soft] FAIL {what}: got {got:#010x}, want {want:#010x}");
        }
    }

    pub fn summary(&self, label: &str) {
        println!(
            "[f32-soft] SUMMARY {label}: {} passed, {} failed",
            self.passed, self.failed
        );
    }
}
