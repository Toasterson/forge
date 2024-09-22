use component::Component;
use miette::{IntoDiagnostic, Report, Result};
use std::fs::{read_to_string, File};
use std::process::Command;
use workspace::Workspace;

const INSTALLED_PACKAGES_FILE: &str = "installed_packages.txt";

fn install_development_dependencies(pkg: &Component) -> Result<()> {
    println!("Installing all development dependencies in one transaction");
    let build_dependencies = pkg.recipe.dependencies
        .iter()
        .filter(|d| d.dev)
        .map(
            |d| d.name.clone()
        )
        .collect::<Vec<String>>();

    let pkg_status = Command::new("pfexec")
        .arg("pkg")
        .arg("install")
        .args(build_dependencies.iter().map(|d|d.as_str()))
        .status()
        .into_diagnostic()?;

    if pkg_status.success() {
        println!("Dependencies Installed");
        Ok(())
    } else {
        Err(miette::miette!("non zero return code from pkg check logs above"))
    }
}

fn get_installed_packages_list(wks: &Workspace) -> Result<()> {
    let installed_package_file = File::create(wks.get_root_path().join(INSTALLED_PACKAGES_FILE))
        .into_diagnostic()?;
    let pkg_status = Command::new("pkg")
        .arg("list")
        .stdout(installed_package_file)
        .status()
        .into_diagnostic()?;

    if pkg_status.success() {
        Ok(())
    } else {
        Err(miette::miette!("non zero return code from pkg list"))
    }
}

pub fn ensure_packages_are_installed(wks: &Workspace, force_refresh: bool, pkg: &Component) -> Result<()> {
    if let Some(stat) = std::fs::metadata(wks.get_root_path().join(INSTALLED_PACKAGES_FILE)).ok() {
        let mod_time = stat.modified().into_diagnostic()?;
        let elapsed = mod_time.elapsed().into_diagnostic()?;
        if elapsed.as_secs() > 240 || !force_refresh {
            get_installed_packages_list(wks)?;
        }
    } else {
        get_installed_packages_list(wks)?;
    }

    let package_list = read_installed_packages_file(wks)?;
    let mut run_install= false;
    for dep in pkg.recipe.dependencies.iter(){
        if dep.dev {
            if !package_list.contains(&dep.name) {
                println!("Package {} not installed", &dep.name);
                run_install = true
            }
        }
    }

    if run_install {
        install_development_dependencies(pkg)?;
    }

    Ok(())
}

fn read_installed_packages_file(wks: &Workspace) -> Result<Vec<String>, Report> {
    let file_contents = read_to_string(wks.get_root_path().join(INSTALLED_PACKAGES_FILE))
        .into_diagnostic()?;
    let installed_packages = file_contents.lines().filter_map(|l| {
        if let Some((pkg_name, _rest)) = l.split_once(" ") {
            Some(pkg_name.to_owned())
        } else {
            None
        }
    }).collect::<Vec<String>>();
    Ok(installed_packages)
}

