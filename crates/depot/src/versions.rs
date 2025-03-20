use std::{collections::HashMap, process::ChildStderr};

use schemars::JsonSchema;
use serde::Serialize;

#[derive(Debug, Serialize, JsonSchema)]
pub struct VersionsResponse {
    interfaces: HashMap<String, Vec<i8>>,
}

impl VersionsResponse {
    pub fn new() -> Self {
        Self {
            interfaces: HashMap::new(),
        }
    }

    pub fn enable(&mut self, name: String, version: i8) {
        if let Some(vers) = self.interfaces.get_mut(&name) {
            if !vers.contains(&version) {
                vers.push(version);
            }
        } else {
            self.interfaces.insert(name, version);
        }
    }

    pub fn wr_str(&self) -> String {
        let mut s = String::new();
        for (iface, versions) in self.interfaces {
            s.push_str(format!("{} {}\n", iface, versions.iter().map(|v| v.to_string()).collect::<Vec<String>().join(" ")).as_str());
        }
    }
}

pub fn get_version_v0() -> String {
    let mut vers = VersionsResponse::new();
    vers.enable("versions".to_string(), 0);
    let mut resp_str = String::from("depot-server develop\n");
    resp_str.push_str(vers.wr_str().as_str());
    resp_str
}
