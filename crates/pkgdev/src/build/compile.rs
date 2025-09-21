use std::{collections::HashMap, process::Stdio};

use crate::sources::derive_source_name;
use component::Component;
use forge_config::Settings;
use miette::{IntoDiagnostic, Result, WrapErr};
use std::process::Command;
use workspace::Workspace;

enum BuildTool {
    Make,
    Ninja,
}

impl std::fmt::Display for BuildTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            BuildTool::Make => "make",
            BuildTool::Ninja => "ninja",
        };
        write!(f, "{}", s)
    }
}

pub fn run_compile(wks: &Workspace, pkg: &Component, settings: &Settings) -> Result<()> {
    let build_dir = wks.get_or_create_build_dir()?;
    let unpack_name = derive_source_name(pkg.recipe.name.clone());

    let unpack_path = build_dir.join(&unpack_name);
    if pkg.recipe.seperate_build_dir {
        let out_dir = build_dir.join("out");
        std::env::set_current_dir(&out_dir)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to change directory to build output at {}",
                    out_dir.display()
                )
            })?;
    } else {
        std::env::set_current_dir(&unpack_path)
            .into_diagnostic()
            .wrap_err_with(|| {
                format!(
                    "failed to change directory to unpack path at {}",
                    unpack_path.display()
                )
            })?;
    }

    let build_tool_check_dir = if pkg.recipe.seperate_build_dir {
        build_dir.join("out")
    } else {
        unpack_path.clone()
    };

    let build_tool = if build_tool_check_dir.join("Makefile").exists() {
        BuildTool::Make
    } else if build_tool_check_dir.join("build.ninja").exists() {
        BuildTool::Ninja
    } else {
        return Err(miette::miette!("no supported build tool could be detected make sure a Makefile or build.ninja file exists in the build directory"));
    };

    let mut env_flags: HashMap<String, String> = HashMap::new();
    env_flags.insert("PATH".into(), settings.get_search_path().join(":"));
    let mut build_cmd = Command::new(build_tool.to_string());
    build_cmd.env_clear();
    build_cmd.envs(&env_flags);

    build_cmd.stdin(Stdio::null());
    build_cmd.stdout(Stdio::inherit());

    tracing::info!(target: "pkgdev::build", "Running {}; env=[{}]",
        build_tool.to_string(),
        env_flags
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<String>>()
            .join(",")
    );

    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "<unknown>".into());
    let status = build_cmd.status().into_diagnostic().wrap_err_with(|| {
        format!(
            "failed to run build tool '{}' in cwd {}",
            build_tool.to_string(),
            cwd
        )
    })?;
    if status.success() {
        tracing::info!(target: "pkgdev::build", "Successfully built {}", pkg.get_name());
    } else {
        return Err(miette::miette!(format!(
            "Could not build {}",
            pkg.get_name()
        )));
    }

    Ok(())
}
