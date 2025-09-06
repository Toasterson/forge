use std::path::{Path, PathBuf};

use cargo_metadata::TargetKind;
use component::{BuildSection, Component, Recipe, RecipeBuilder};
use miette::{IntoDiagnostic, WrapErr};

#[derive(Debug, Clone)]
pub struct CargoDerivedComponent {
    pub component: Component,
    pub root_dir: PathBuf,
}

fn first_non_empty<'a>(a: Option<&'a String>, b: Option<&'a String>) -> Option<String> {
    a.cloned().filter(|s| !s.is_empty()).or_else(|| b.cloned())
}

fn build_component_from_package(
    pkg: &cargo_metadata::Package,
    workspace_meta: &serde_json::Value,
    manifest_dir: &Path,
) -> miette::Result<Component> {
    let name = pkg.name.clone();
    let version = Some(pkg.version.to_string());

    let description = if pkg
        .description
        .as_ref()
        .map(|s| s.trim().is_empty())
        .unwrap_or(true)
    {
        None
    } else {
        pkg.description.clone()
    };
    let homepage = first_non_empty(pkg.homepage.as_ref(), pkg.repository.as_ref());
    let license = pkg.license.clone();

    // Optional forge metadata in [package.metadata.forge] and workspace-level defaults
    let mut classification: Option<String> = None;
    let mut fmri: Option<String> = None;

    // Package-level forge metadata
    if let Some(serde_json::Value::Object(meta)) = pkg.metadata.get("forge") {
        if let Some(serde_json::Value::String(cls)) = meta.get("classification") {
            if !cls.is_empty() {
                classification = Some(cls.clone());
            }
        }
        if let Some(serde_json::Value::String(fm)) = meta.get("fmri") {
            if !fm.is_empty() {
                fmri = Some(fm.clone());
            }
        }
    }

    // Workspace-level forge metadata fallbacks
    if classification.is_none() || fmri.is_none() {
        if let Some(serde_json::Value::Object(ws_forge)) = workspace_meta.get("forge") {
            if classification.is_none() {
                if let Some(serde_json::Value::String(cls)) = ws_forge.get("classification") {
                    if !cls.is_empty() {
                        classification = Some(cls.clone());
                    }
                }
            }
            if fmri.is_none() {
                if let Some(serde_json::Value::String(base)) = ws_forge.get("fmri") {
                    if !base.is_empty() {
                        let base_trim = base.trim_end_matches('/');
                        // Store only the FMRI base (stem); full package name will be composed later
                        fmri = Some(base_trim.to_string());
                    }
                }
            }
        }
    }

    // Build Recipe
    let mut builder = RecipeBuilder::default();
    builder.name(name.clone().to_string());
    if let Some(ver) = version.clone() {
        builder.version(ver);
    }
    // Ensure summary is always present: use Cargo description or fallback to crate name
    builder.summary(description.clone().unwrap_or_else(|| name.to_string()));
    if let Some(cls) = &classification {
        builder.classification(cls.clone());
    }
    // Ensure project_url is set; prefer homepage/repository, else default to crates.io page
    let project_url = homepage.unwrap_or_else(|| format!("https://crates.io/crates/{}", name));
    builder.project_url(project_url);
    if let Some(lic) = &license {
        builder.license(lic.clone());
    }

    let recipe: Recipe = builder
        .build()
        .into_diagnostic()
        .wrap_err("failed to build recipe from cargo metadata")?;

    let mut comp = Component::new(name.to_string(), Some(manifest_dir))
        .into_diagnostic()
        .wrap_err("failed to initialize component for cargo project")?;
    comp.recipe = recipe;
    comp.recipe.build_sections.push(BuildSection {
        cargo: true,
        ..Default::default()
    });
    if let Some(f) = fmri {
        comp.recipe.insert_metadata("fmri", &f);
    }

    Ok(comp)
}

/// Attempt to build a Component from a Cargo project directory (with Cargo.toml).
/// This uses cargo_metadata to detect the package/workspace and map fields into a Recipe.
pub fn component_from_cargo_dir(dir: &Path) -> miette::Result<CargoDerivedComponent> {
    // Use cargo_metadata to read metadata
    let mut cmd = cargo_metadata::MetadataCommand::new();
    cmd.current_dir(dir);
    let metadata = cmd.exec().into_diagnostic().wrap_err_with(|| {
        format!(
            "failed to run cargo metadata in {} — is Cargo.toml valid?",
            dir.display()
        )
    })?;

    let dir_can = dir
        .canonicalize()
        .into_diagnostic()
        .wrap_err("failed to canonicalize cargo project path")?;

    let workspace_meta = metadata.workspace_metadata.clone();

    // Prefer a package whose manifest dir is exactly the root dir
    let mut primary_pkg = None;
    for p in &metadata.packages {
        let mp = PathBuf::from(p.manifest_path.as_str());
        if let Ok(mp_dir) = mp.parent().unwrap_or(Path::new(".")).canonicalize() {
            if mp_dir == dir_can {
                primary_pkg = Some(p);
                break;
            }
        }
    }
    // Fallback: first package under the dir
    if primary_pkg.is_none() {
        for p in &metadata.packages {
            let mp = PathBuf::from(&p.manifest_path);
            if let Ok(mp_dir) = mp.parent().unwrap_or(Path::new(".")).canonicalize() {
                if mp_dir.starts_with(&dir_can) {
                    primary_pkg = Some(p);
                    break;
                }
            }
        }
    }

    let pkg = primary_pkg
        .ok_or_else(|| miette::miette!("no cargo package found under {}", dir.display()))?;

    let manifest_dir = PathBuf::from(pkg.manifest_path.as_str())
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();

    let component = build_component_from_package(pkg, &workspace_meta, &manifest_dir)?;

    Ok(CargoDerivedComponent {
        component,
        root_dir: dir_can,
    })
}

/// Build Components for all packages in a Cargo workspace located at `dir`.
pub fn components_from_cargo_dir(dir: &Path) -> miette::Result<Vec<CargoDerivedComponent>> {
    let mut cmd = cargo_metadata::MetadataCommand::new();
    cmd.current_dir(dir);
    let metadata = cmd.exec().into_diagnostic().wrap_err_with(|| {
        format!(
            "failed to run cargo metadata in {} — is Cargo.toml valid?",
            dir.display()
        )
    })?;

    let dir_can = dir
        .canonicalize()
        .into_diagnostic()
        .wrap_err("failed to canonicalize cargo project path")?;

    let workspace_meta = metadata.workspace_metadata.clone();

    let mut out = Vec::new();
    for p in &metadata.packages {
        let mp = PathBuf::from(p.manifest_path.as_str());
        let mp_dir = match mp.parent() {
            Some(d) => d.to_path_buf(),
            None => PathBuf::from("."),
        };
        let mp_dir_can = mp_dir.canonicalize().unwrap_or(mp_dir.clone());
        if !mp_dir_can.starts_with(&dir_can) {
            continue;
        }
        // Only include packages that produce at least one binary target
        let has_bin = p
            .targets
            .iter()
            .any(|t| t.kind.iter().any(|k| *k == TargetKind::Bin));
        if !has_bin {
            continue;
        }
        let comp = build_component_from_package(p, &workspace_meta, &mp_dir_can)?;
        out.push(CargoDerivedComponent {
            component: comp,
            root_dir: dir_can.clone(),
        });
    }

    Ok(out)
}
