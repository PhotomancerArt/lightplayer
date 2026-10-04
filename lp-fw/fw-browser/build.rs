//! Stamps the browser runtime with this build's app version
//! (`LP_APP_VERSION`), which its manifest core and its wire hello report.

fn main() {
    lp_app_version::emit();
    // The version helper's watches turn off cargo's default rerun rule;
    // nothing else here needs one.
    println!("cargo:rerun-if-changed=build.rs");
}
