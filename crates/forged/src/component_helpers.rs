use crate::{prisma, Error, Result};
use itertools::Itertools;
use prisma::component::Data as DatabaseComponent;
use semver::{BuildMetadata, Version};
use std::collections::HashMap;

pub fn find_latest_component_in_set(set: Vec<DatabaseComponent>) -> Result<DatabaseComponent> {
    let mut versions: HashMap<Version, DatabaseComponent> = HashMap::new();
    for item in set.iter() {
        let mut version_parsed: Version = item.version.parse()?;
        version_parsed.build = BuildMetadata::new(&item.revision)
            .map_err(|_|Error::NoRevisionFoundInRecipe(item.name.clone()))?;

        versions.insert(version_parsed, item.clone());
    }

    let binding = versions
        .iter()
        .sorted_by_key(|(&ref version, _)| version.clone())
        .map(|(_, component)| component.clone())
        .collect::<Vec<DatabaseComponent>>();

    let comp = binding.first();

    comp.map(|data| data.clone()).ok_or(Error::NoComponentFound)
}
