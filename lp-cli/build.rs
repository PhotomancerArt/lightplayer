//! Stamps `lp-cli` with this build's app version (`LP_APP_VERSION`), which
//! `lp-cli --version` prints.

fn main() {
    lp_app_version::emit();
    // The version helper's watches turn off cargo's default rerun rule;
    // nothing else here needs one.
    println!("cargo:rerun-if-changed=build.rs");
}
