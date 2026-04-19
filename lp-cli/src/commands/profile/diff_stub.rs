use std::process;

use super::args::ProfileDiffArgs;

pub fn handle_profile_diff(args: ProfileDiffArgs) -> ! {
    eprintln!("error: 'lp-cli profile diff' is not yet implemented (planned for cpu-profile m2)");
    eprintln!("trace dirs: {}, {}", args.a.display(), args.b.display());
    process::exit(2);
}
