mod common;

use pkgdev::args::*;
use pkgdev::build::BuildArgs;
use std::path::PathBuf;

// #[tokio::test]
// async fn test_build_ffmpeg() -> miette::Result<()> {
//     let run_command = Commands::Build {
//         component: PathBuf::from("../../../sample_data/components/encumbered/components/ffmpeg"),
//         args: BuildArgs{
//             stop_on_step: None,
//             no_clean: false,
//             archive_clean: false,
//             transform_include_dir: None,
//         },
//     };
//     let run_args = Args {
//         gate: None,
//         workspace: None,
//         command: run_command,
//     };
//     run(run_args).await
// }
