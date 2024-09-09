use std::{
    collections::HashMap,
    fs::DirBuilder

    ,
    process::{Command, Stdio},
};

use component::{Component, ConfigureBuildSection};
use miette::{IntoDiagnostic, Result, WrapErr};
use workspace::Workspace;
use config::Settings;
use crate::sources::derive_source_name;

pub fn build_using_automake(
    wks: &Workspace,
    pkg: &Component,
    build_section: &ConfigureBuildSection,
    settings: &Settings,
) -> Result<()> {
    let build_dir = wks.get_or_create_build_dir()?;
    let unpack_name = derive_source_name(
        pkg.recipe.name.clone(),
    );
    let unpack_path = build_dir.join(&unpack_name);
    if pkg.recipe.seperate_build_dir {
        let out_dir = build_dir.join("out");
        DirBuilder::new().create(&out_dir).into_diagnostic()?;
        std::env::set_current_dir(&out_dir).into_diagnostic()?;
    } else {
        std::env::set_current_dir(&unpack_path).into_diagnostic()?;
    }

    let mut option_vec: Vec<_> = vec![];
    let mut env_flags: HashMap<String, String> = HashMap::new();

    for option in build_section.options.iter() {
        let opt_arg = format!("--{}", option.option);
        option_vec.push(opt_arg);
    }

    for flag in build_section.flags.iter() {
        let flag_value = crate::build::util::expand_env(&flag.flag)?;

        if let Some(flag_name) = &flag.flag_name {
            let flag_name = flag_name.to_uppercase();
            if env_flags.contains_key(&flag_name) {
                let flag_ref = env_flags.get_mut(&flag_name).unwrap();
                flag_ref.push_str(" ");
                flag_ref.push_str(&flag_value);
            } else {
                env_flags.insert(flag_name, flag_value.clone());
            }
        } else {
            for flag_name in vec![
                String::from("CFLAGS"),
                String::from("CXXFLAGS"),
                String::from("CPPFLAGS"),
                String::from("FFLAGS"),
            ] {
                if env_flags.contains_key(&flag_name) {
                    let flag_ref = env_flags.get_mut(&flag_name).unwrap();
                    flag_ref.push_str(" ");
                    flag_ref.push_str(&flag_value);
                } else {
                    env_flags.insert(flag_name, flag_value.clone());
                }
            }
        }
    }

    if let Some(prefix) = &pkg.recipe.prefix {
        option_vec.push(format!("--prefix={}", prefix));
    }

    env_flags.insert("PATH".into(), settings.get_search_path().join(":"));
    let proto_dir_path = wks.get_or_create_prototype_dir()?;
    let proto_dir_str = proto_dir_path.to_string_lossy().to_string();

    env_flags.insert(String::from("DESTDIR"), proto_dir_str.clone());
    let destdir_arg = format!("DESTDIR={}", &proto_dir_str);

    let bin_path = if pkg.recipe.seperate_build_dir {
        unpack_path.join("configure").to_string_lossy().to_string()
    } else {
        String::from("./configure")
    };

    let mut configure_cmd = Command::new(&bin_path);
    configure_cmd.env_clear();
    configure_cmd.envs(&env_flags);
    configure_cmd.args(&option_vec);
    if !build_section.disable_destdir_configure_option {
        println!("DESTDIR option not injecting into configure script options");
        configure_cmd.arg(&destdir_arg);
    }

    configure_cmd.stdin(Stdio::null());
    configure_cmd.stdout(Stdio::inherit());

    println!(
        "Running configure with options {}; {}; env=[{}]",
        option_vec.join(" "),
        destdir_arg,
        env_flags
            .into_iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<String>>()
            .join(",")
    );

    let status = configure_cmd.status().into_diagnostic()?;
    if status.success() {
        println!("Successfully configured {}", pkg.get_name());
    } else {
        return Err(miette::miette!(format!(
            "Could not configure {}",
            pkg.get_name()
        )));
    }

    crate::build::compile::run_compile(wks, pkg, settings).wrap_err("compilation step failed")?;

    crate::build::install::run_install(wks, pkg, settings).wrap_err("installation step failed")
}