use std::fs;
use std::path::PathBuf;

use clap::{arg, Subcommand, ValueEnum};
use miette::IntoDiagnostic;

use crate::component::open_component_local;
use component::{
    ArchiveSourceBuilder, BuildFlagNode, BuildOptionNode, BuildSectionBuilder,
    ConfigureBuildSection, DependencyBuilder, DependencyKind, ScriptBuildSection, ScriptNode,
    SourceNode, SourceSection,
};
use gate::Gate;

#[derive(Debug, Subcommand)]
pub enum EditArgs {
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
pub enum SetArgs {
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
        #[clap(short, long)]
        file: String,
    },
    Version {
        arg: String,
    },
    ProjectUrl {
        arg: String,
    },
    Metadata {
        key: String,
        value: String,
    },
    Build {
        #[arg(short, long, value_parser)]
        index: usize,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true, raw = true)]
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
pub enum AddArgs {
    Dependency {
        #[clap(long)]
        dev: bool,
        #[clap(long)]
        kind: String,
        package: String,
    },
    Build {
        kind: BuildKind,
        /// Environment variables to set at configure time, in the form KEY=VALUE. Repeatable.
        #[arg(long = "env")]
        env: Vec<String>,
        /// Configure options to pass (without leading --). Collected as trailing args.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
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
pub enum SourceKind {
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
pub enum BuildKind {
    Configure,
    Script,
    Cmake,
    Meson,
    #[default]
    None,
}

pub fn edit_component(
    component_path: PathBuf,
    gate: Option<Gate>,
    args: EditArgs,
) -> miette::Result<()> {
    let mut c = open_component_local(component_path, &gate)?;
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
            AddArgs::Build { kind, env, args } => {
                let mut bsb = BuildSectionBuilder::default();
                match kind {
                    BuildKind::Configure => {
                        // Build Configure section with provided env and options
                        let mut cfg = ConfigureBuildSection {
                            options: vec![],
                            flags: vec![],
                            compiler: None,
                            linker: None,
                            disable_destdir_configure_option: false,
                            enable_large_files: false,
                        };

                        // Parse and add env KEY=VALUE as flags with names
                        for e in env {
                            if let Some((k, v)) = e.split_once('=') {
                                cfg.flags.push(BuildFlagNode {
                                    flag: v.to_string(),
                                    flag_name: Some(k.to_string()),
                                });
                            } else {
                                return Err(miette::miette!(format!(
                                    "invalid --env format (expected KEY=VALUE): {}",
                                    e
                                )));
                            }
                        }

                        // Add configure options from trailing args, applying gate transforms if present
                        'outer: for arg in args {
                            let raw_arg = arg.clone();
                            let normalized_arg = raw_arg.trim_start_matches('-').to_string();
                            if let Some(gate) = &gate {
                                for transform in &gate.metadata_transforms {
                                    let matcher = &transform.matcher;
                                    let norm_matcher = matcher.trim_start_matches('-');
                                    if raw_arg.contains(matcher)
                                        || normalized_arg.contains(norm_matcher)
                                    {
                                        if !transform.drop {
                                            let replacement = transform
                                                .replacement
                                                .trim_start_matches('-')
                                                .to_string();
                                            println!(
                                                "replacing {} with {}",
                                                &raw_arg, &replacement
                                            );
                                            cfg.options.push(BuildOptionNode {
                                                option: replacement,
                                            });
                                        } else {
                                            println!("dropping {}", &raw_arg);
                                        }
                                        continue 'outer;
                                    }
                                }
                            }
                            // Store option without leading dashes to avoid doubling at build time
                            cfg.options.push(BuildOptionNode {
                                option: normalized_arg,
                            });
                        }

                        bsb.configure(cfg);
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
            AddArgs::Source { args } => {
                if c.recipe.sources.is_empty() {
                    c.recipe.sources.push(SourceSection { sources: vec![] });
                }
                let src_section = c.recipe.sources.first_mut().unwrap();
                match args {
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
                        src_section.sources.push(SourceNode::Archive(archive));
                    }
                    SourceKind::Patch { path_dir } => {
                        let mut patch_vec = vec![];
                        let read_dir_res = fs::read_dir(&path_dir).into_diagnostic()?;
                        for entry in read_dir_res {
                            let entry = entry.into_diagnostic()?;
                            if entry.path().is_file()
                                && entry.path().extension().is_some_and(|t| t == "patch")
                            {
                                let file_name = entry
                                    .path()
                                    .file_name()
                                    .ok_or(miette::miette!(
                                        "no filename for {}",
                                        &path_dir.display()
                                    ))?
                                    .to_owned()
                                    .to_string_lossy()
                                    .to_string();
                                patch_vec.push(SourceNode::Patch(component::PatchSource::new(
                                    file_name, None,
                                )));
                            }
                            src_section.sources.append(&mut patch_vec);
                        }
                    }
                    SourceKind::File => {}
                    SourceKind::Git => {}
                }
            }
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
            SetArgs::License { arg, file } => {
                c.recipe.license = Some(arg);
                c.recipe.license_file = Some(file);
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
                        'outer: for arg in args {
                            let raw_arg = arg.clone();
                            let normalized_arg = raw_arg.trim_start_matches('-').to_string();
                            if let Some(gate) = &gate {
                                for transform in &gate.metadata_transforms {
                                    let matcher = &transform.matcher;
                                    let norm_matcher = matcher.trim_start_matches('-');
                                    if raw_arg.contains(matcher)
                                        || normalized_arg.contains(norm_matcher)
                                    {
                                        if !transform.drop {
                                            let replacement = transform
                                                .replacement
                                                .trim_start_matches('-')
                                                .to_string();
                                            println!(
                                                "replacing {} with {}",
                                                &raw_arg, &replacement
                                            );
                                            configure.options.push(BuildOptionNode {
                                                option: replacement,
                                            });
                                        } else {
                                            println!("dropping {}", &raw_arg);
                                        }
                                        continue 'outer;
                                    }
                                }
                            }
                            // Store option without leading dashes to avoid doubling at build time
                            configure.options.push(BuildOptionNode {
                                option: normalized_arg,
                            });
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
            SetArgs::Metadata { key, value } => {
                c.recipe.insert_metadata(&key, &value);
            }
        },
    }
    c.save_document()?;
    Ok(())
}
