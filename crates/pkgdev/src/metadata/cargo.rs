use std::path::{Path, PathBuf};

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

    // Choose the primary package: if there is a package at the root, prefer it;
    // otherwise pick the first workspace member with a manifest inside the dir.
    let dir_can = dir
        .canonicalize()
        .into_diagnostic()
        .wrap_err("failed to canonicalize cargo project path")?;

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
    // Fallback: first package whose manifest is under dir tree
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

    let pkg = primary_pkg.ok_or_else(|| {
        miette::miette!(
            "no cargo package found under {} — workspaces without a root package are not yet supported",
            dir.display()
        )
    })?;

    // Map fields
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
        if let Some(serde_json::Value::Object(ws_forge)) = metadata.workspace_metadata.get("forge")
        {
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
                        fmri = Some(format!("{}/{}", base_trim, name));
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
    if let Some(desc) = &description {
        builder.summary(desc.clone());
    }
    if let Some(cls) = &classification {
        builder.classification(cls.clone());
    }
    if let Some(url) = &homepage {
        builder.project_url(url.clone());
    }
    if let Some(lic) = &license {
        builder.license(lic.clone());
    }

    // Create a minimal Component with no sources/build sections (cargo builder handles build)
    let recipe: Recipe = builder
        .build()
        .into_diagnostic()
        .wrap_err("failed to build recipe from cargo metadata")?;

    let mut comp = Component::new(name.to_string(), Some(dir))
        .into_diagnostic()
        .wrap_err("failed to initialize component for cargo project")?;
    // Overwrite the auto-created recipe with our metadata-based recipe
    comp.recipe = recipe;
    // Ensure we have a cargo build section so the build flow uses the cargo branch
    comp.recipe.build_sections.push(BuildSection {
        cargo: true,
        ..Default::default()
    });
    // If fmri was discovered, store it in recipe metadata so downstream (IPS) can use it
    if let Some(f) = fmri {
        comp.recipe.insert_metadata("fmri", &f);
    }

    Ok(CargoDerivedComponent {
        component: comp,
        root_dir: dir_can,
    })
}
