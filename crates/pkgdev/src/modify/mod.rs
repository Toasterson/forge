use clap::{arg, Subcommand, ValueEnum};
use component::{
    ArchiveSourceBuilder, BuildOptionNode, BuildSectionBuilder, Component, ConfigureBuildSection,
    DependencyBuilder, DependencyKind, ScriptBuildSection, ScriptNode, SourceNode, SourceSection,
};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub(crate) enum EditArgs {
    Add {
        #[clap(subcommand)]
        args: AddArgs,
    },
    Set {
        #[clap(subcommand)]
        args: SetArgs,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SetArgs {
    ProjectName {
        arg: String,
    },
    Summary {
        arg: String,
    },
    Classification {
        arg: String,
    },
    License {
        arg: String,
    },
    Version {
        arg: String,
    },
    ProjectUrl {
        arg: String,
    },
    Build {
        index: usize,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// A dummy command that accepts any arguments
#[derive(clap::Args, Clone, Debug)]
struct DummyCommandArgs {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

#[derive(Debug, Subcommand, Clone)]
pub(crate) enum AddArgs {
    Dependency {
        #[clap(long)]
        dev: bool,
        #[clap(long)]
        kind: String,
        package: String,
    },
    Build {
        kind: BuildKind,
    },
    Source {
        #[clap(subcommand)]
        args: SourceKind,
    },
    Maintainer {
        arg: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub(crate) enum SourceKind {
    Archive {
        source_url: url::Url,
        source_hash: String,
    },
    Patch {
        path_dir: PathBuf,
    },
    File,
    Git,
}

#[derive(Debug, Clone, ValueEnum, Default)]
pub(crate) enum BuildKind {
    Configure,
    Script,
    Cmake,
    Meson,
    #[default]
    None,
}

pub(crate) fn edit_component(component_path: PathBuf, args: EditArgs) -> miette::Result<()> {
    let mut c = Component::open_local(component_path)?;
    match args {
        EditArgs::Add { args } => match args {
            AddArgs::Dependency { dev, kind, package } => {
                let dep = DependencyBuilder::default()
                    .name(package)
                    .kind(DependencyKind::from(kind.as_str()))
                    .dev(dev)
                    .build()?;
                c.recipe.dependencies.push(dep);
            }
            AddArgs::Build { kind } => {
                let mut bsb = BuildSectionBuilder::default();
                match kind {
                    BuildKind::Configure => {
                        bsb.configure(ConfigureBuildSection {
                            options: vec![],
                            flags: vec![],
                            compiler: None,
                            linker: None,
                        });
                    }
                    BuildKind::Script => {
                        bsb.script(ScriptBuildSection {
                            scripts: vec![],
                            install_directives: vec![],
                        });
                    }
                    BuildKind::Cmake => {
                        bsb.cmake("");
                    }
                    BuildKind::Meson => {
                        bsb.meson("");
                    }
                    BuildKind::None => {}
                }
                c.recipe.build_sections.push(bsb.build()?);
            }
            AddArgs::Source { args } => match args {
                SourceKind::Archive {
                    source_url,
                    source_hash,
                } => {
                    let mut archive = ArchiveSourceBuilder::default();
                    archive.src(source_url);
                    if let Some((kind, hash)) = source_hash.split_once(':') {
                        match kind {
                            "sha256" => {
                                archive.sha256(hash);
                            }
                            "sha512" => {
                                archive.sha512(hash);
                            }
                            _ => {}
                        }
                    }
                    let archive = archive.build()?;
                    if let Some(src) = c.recipe.sources.first_mut() {
                        src.sources.push(SourceNode::Archive(archive));
                    } else {
                        c.recipe.sources.push(SourceSection {
                            sources: vec![SourceNode::Archive(archive)],
                        });
                    };
                }
                SourceKind::Patch { path_dir } => {}
                SourceKind::File => {}
                SourceKind::Git => {}
            },
            AddArgs::Maintainer { arg } => {
                c.recipe.maintainers.push(arg);
            }
        },
        EditArgs::Set { args } => match args {
            SetArgs::ProjectName { arg } => {
                c.recipe.project_name = Some(arg);
            }
            SetArgs::Summary { arg } => {
                c.recipe.summary = Some(arg);
            }
            SetArgs::Classification { arg } => {
                c.recipe.classification = Some(arg);
            }
            SetArgs::License { arg } => {
                c.recipe.license = Some(arg);
            }
            SetArgs::Version { arg } => {
                c.recipe.version = Some(arg);
            }
            SetArgs::ProjectUrl { arg } => {
                c.recipe.project_url = Some(arg);
            }
            SetArgs::Build { index, args } => {
                if let Some(section) = c.recipe.build_sections.get_mut(index) {
                    if let Some(configure) = &mut section.configure {
                        for arg in args {
                            configure.options.push(BuildOptionNode { option: arg });
                        }
                    } else if let Some(script) = &mut section.script {
                        for arg in args {
                            script.scripts.push(ScriptNode {
                                name: arg,
                                prototype_dir: None,
                            });
                        }
                    } else {
                        return Err(miette::miette!(
                            "cannot add arguments to an empty build section"
                        ));
                    }
                } else {
                    return Err(miette::miette!("No build section for that index"));
                }
            }
        },
    }
    c.save_document()?;
    Ok(())
}
