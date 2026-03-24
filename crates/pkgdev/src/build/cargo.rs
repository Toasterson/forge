use std::fs;
use std::process::{Command, Stdio};

use component::{CargoBuildSection, Component};
use miette::{IntoDiagnostic, Result, WrapErr};
use workspace::Workspace;

/// Build and stage a Cargo project using settings from `CargoBuildSection`.
///
/// This runs `cargo build --release` with the configured packages, features,
/// target triple, and environment variables, then installs the resulting
/// binaries into the prototype area via `cargo install`.
pub fn build_and_stage_cargo(
    wks: &Workspace,
    component: &Component,
    cargo_config: &CargoBuildSection,
) -> Result<()> {
    let root = component.get_path();

    // --- Build phase ---
    tracing::info!(target: "pkgdev::cargo", "[cargo] Building project at {}", root.display());

    let mut build_cmd = Command::new("cargo");
    build_cmd.current_dir(root);
    build_cmd.arg("build");
    build_cmd.arg("--release");

    // Add package selections
    for pkg in &cargo_config.packages {
        build_cmd.arg("-p");
        build_cmd.arg(pkg);
    }

    // Add features
    if !cargo_config.features.is_empty() {
        build_cmd.arg("--features");
        build_cmd.arg(cargo_config.features.join(","));
    }

    // Offline mode
    if cargo_config.offline {
        build_cmd.arg("--offline");
    }

    // Locked mode
    if cargo_config.locked {
        build_cmd.arg("--locked");
    }

    // Cross-compilation target
    if let Some(target) = &cargo_config.target {
        build_cmd.arg("--target");
        build_cmd.arg(target);
    }

    // Merge all env var nodes and apply them
    let merged_env = merge_env_vars(cargo_config);
    for (key, value) in &merged_env {
        tracing::debug!(target: "pkgdev::cargo", "[cargo] env {}={}", key, value);
        build_cmd.env(key, value);
    }

    build_cmd.stdout(Stdio::inherit());
    build_cmd.stderr(Stdio::inherit());

    let status = build_cmd
        .status()
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to spawn `cargo build` in {}\n\nhelp: ensure `cargo` is on PATH and the project has a valid Cargo.toml",
                root.display()
            )
        })?;

    if !status.success() {
        let code = status
            .code()
            .map_or("unknown".to_string(), |c| c.to_string());
        return Err(miette::miette!(
            help = "check the compiler output above for errors",
            "cargo build exited with status {} in {}",
            code,
            root.display()
        ));
    }

    // --- Install / stage phase ---
    let proto_dir = wks
        .get_or_create_prototype_dir()
        .wrap_err("failed to get prototype directory")?;

    let install_root_suffix = cargo_config.install_root.as_deref().unwrap_or("/usr");

    // Strip leading slash so the join works correctly
    let install_root_suffix = install_root_suffix
        .strip_prefix('/')
        .unwrap_or(install_root_suffix);
    let install_prefix = proto_dir.join(install_root_suffix);

    fs::create_dir_all(&install_prefix)
        .into_diagnostic()
        .wrap_err_with(|| {
            format!(
                "failed to create install root directory {}",
                install_prefix.display()
            )
        })?;

    tracing::info!(
        target: "pkgdev::cargo",
        "[cargo] Installing to proto area at {}",
        install_prefix.display()
    );

    let mut install_cmd = Command::new("cargo");
    install_cmd.current_dir(root);
    install_cmd.arg("install");
    install_cmd.arg("--path");
    install_cmd.arg(".");
    install_cmd.arg("--root");
    install_cmd.arg(&install_prefix);
    install_cmd.arg("--force");
    install_cmd.arg("--no-track");

    // For install, replicate the same package selections
    for pkg in &cargo_config.packages {
        install_cmd.arg("--bin");
        install_cmd.arg(pkg);
    }

    // Replicate features
    if !cargo_config.features.is_empty() {
        install_cmd.arg("--features");
        install_cmd.arg(cargo_config.features.join(","));
    }

    // Offline + locked
    if cargo_config.offline {
        install_cmd.arg("--offline");
    }
    if cargo_config.locked {
        install_cmd.arg("--locked");
    }

    // Cross-compilation target
    if let Some(target) = &cargo_config.target {
        install_cmd.arg("--target");
        install_cmd.arg(target);
    }

    // Apply same environment variables
    for (key, value) in &merged_env {
        install_cmd.env(key, value);
    }

    install_cmd.stdout(Stdio::inherit());
    install_cmd.stderr(Stdio::inherit());

    let install_status = install_cmd.status().into_diagnostic().wrap_err_with(|| {
        format!(
            "failed to spawn `cargo install` in {}\n\nhelp: ensure `cargo` is on PATH",
            root.display()
        )
    })?;

    if !install_status.success() {
        let code = install_status
            .code()
            .map_or("unknown".to_string(), |c| c.to_string());
        return Err(miette::miette!(
            help = "check the cargo install output above for errors",
            "cargo install exited with status {} in {}",
            code,
            root.display()
        ));
    }

    tracing::info!(
        target: "pkgdev::cargo",
        "[cargo] Staged binaries into {}",
        install_prefix.display()
    );
    Ok(())
}

/// Merge all `EnvVarNode` entries from the cargo config into a single map.
/// Later nodes overwrite earlier ones for the same key.
fn merge_env_vars(cargo_config: &CargoBuildSection) -> Vec<(String, String)> {
    use std::collections::HashMap;
    let mut merged: HashMap<String, String> = HashMap::new();
    for env_node in &cargo_config.env_vars {
        for (key, value) in &env_node.vars {
            merged.insert(key.clone(), value.clone());
        }
    }
    let mut pairs: Vec<(String, String)> = merged.into_iter().collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}
