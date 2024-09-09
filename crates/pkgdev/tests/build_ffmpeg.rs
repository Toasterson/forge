mod common;

use std::path::PathBuf;
use pkgdev::args::*;
use pkgdev::build::BuildArgs;

#[test]
fn test_build_ffmpeg() {
    let run_command = Commands::Build {
        component: PathBuf::from("../../../sample_data/components/encumbered/components/ffmpeg"),
        args: BuildArgs { workspace: None },
    };
    let run_args = Args {
        gate: None,
        command: (),
    };
    run(run_args)
}