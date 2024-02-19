use miette::Diagnostic;
use schemars::{schema_for, JsonSchema};
use serde::{Deserialize, Serialize};
use std::{fs::read_to_string, path::Path};
use thiserror::Error;

#[derive(Error, Debug, Diagnostic)]
pub enum IntegrationError {
    #[error("{0} is not a supported manifest format extention. Use toml,yaml,json or kdl")]
    NotSupportedFormat(String),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    TomlDe(#[from] toml::de::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ForgeIntegrationManifest {
    #[serde(rename = "component_list")]
    pub component_list_script: Vec<String>,
    #[serde(rename = "metadata_generation_script")]
    pub component_metadata_gen_script: Vec<String>,
    #[serde(rename = "metadata_filename", default = "default_metadata_filename")]
    pub component_metadata_filename: String,
    pub change_to_component_dir: bool,
}

fn default_metadata_filename() -> String {
    "package.kdl".to_string()
}

pub type Result<T> = miette::Result<T, IntegrationError>;

pub fn read_forge_manifest<P: AsRef<Path>>(path: P) -> Result<ForgeIntegrationManifest> {
    let path = path.as_ref();
    let contents = read_to_string(path)?;
    let config: ForgeIntegrationManifest = match path
        .extension()
        .ok_or(IntegrationError::NotSupportedFormat(String::from("None")))?
        .to_string_lossy()
        .to_string()
        .as_str()
    {
        "toml" => {
            let fm = toml::from_str(&contents)?;
            Ok(fm)
        }
        "yaml" | "yml" => {
            let fm = serde_yaml::from_str(&contents)?;
            Ok(fm)
        }
        "json" => {
            let fm = serde_json::from_str(&contents)?;
            Ok(fm)
        }
        other => Err(IntegrationError::NotSupportedFormat(other.to_owned())),
    }?;
    Ok(config)
}

pub fn emit_schema() -> Result<String> {
    let schema = schema_for!(ForgeIntegrationManifest);
    Ok(serde_json::to_string_pretty(&schema)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_test() -> Result<()> {
        let schema = emit_schema()?;
        println!("{}", schema);
        assert!(!schema.is_empty());
        Ok(())
    }

    #[test]
    fn toml_test() -> Result<()> {
        let result = read_forge_manifest("./examples/oi-userland.toml")?;
        assert_eq!(result.component_list_script.len(), 1);
        assert_eq!(
            result.component_list_script[0].as_str(),
            "./tools/bass-o-matic --workspace=./ --components=paths | sed -e 's;./components/;;'"
        );
        Ok(())
    }

    #[test]
    fn yaml_test() -> Result<()> {
        let result = read_forge_manifest("./examples/oi-userland.yaml")?;
        assert_eq!(result.component_list_script.len(), 1);
        assert_eq!(
            result.component_list_script[0].as_str(),
            "./tools/bass-o-matic --workspace=./ --components=paths | sed -e 's;./components/;;'"
        );
        Ok(())
    }

    #[test]
    fn json_test() -> Result<()> {
        let result = read_forge_manifest("./examples/oi-userland.json")?;
        assert_eq!(result.component_list_script.len(), 1);
        assert_eq!(
            result.component_list_script[0].as_str(),
            "./tools/bass-o-matic --workspace=./ --components=paths | sed -e 's;./components/;;'"
        );
        Ok(())
    }
}
