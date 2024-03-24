use std::fmt::{Display, Formatter};

use async_graphql::{
    Context, Enum, InputObject, Object, OneofObject, Result, SimpleObject, Upload,
};
use sha3::Digest;
use tracing::trace;
use url::Url;
use crate::graphql::types::{component_from_database, Component, ComponentData};
use crate::{
    prisma::{self},
    Error, SharedState,
};

#[derive(Debug, InputObject)]
pub struct ComponentInput {
    pub data: ComponentData,
    pub anitya_id: Option<String>,
    pub repology_id: Option<String>,
    pub gate: String,
}

#[derive(Debug, InputObject)]
pub struct ComponentsIdentifier {
    pub name: String,
    pub version: String,
    pub revision: String,
    pub gate_id: String,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(remote = "forge::ComponentFileKind")]
pub enum ComponentFileKind {
    Archive,
    Patch,
    Script,
}

impl Display for ComponentFileKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            ComponentFileKind::Archive => "archive",
            ComponentFileKind::Patch => "patch",
            ComponentFileKind::Script => "script",
        };
        write!(f, "{name}")
    }
}

#[derive(OneofObject)]
pub enum UrlOrFile {
    Url(String),
    Upload(Upload),
}

#[derive(Default)]
pub struct ComponentMutation;

#[derive(SimpleObject, Default)]
pub struct Empty {
    pub success: bool,
}

#[Object]
impl ComponentMutation {
    async fn create_component(
        &self,
        ctx: &Context<'_>,
        input: ComponentInput,
    ) -> Result<Component> {
        let db = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let encoded_recipe = serde_json::to_value(&input.data.recipe)?;
        let encoded_package_meta = serde_json::to_value(&input.data.packages)?;

        let component =
            db.component()
                .create(
                    input.data.recipe.name.clone(),
                    input
                        .data
                        .recipe
                        .version
                        .clone()
                        .ok_or(Error::NoVersionFoundInRecipe(
                            input.data.recipe.name.clone(),
                        ))?,
                    input
                        .data
                        .recipe
                        .revision
                        .clone()
                        .unwrap_or(String::from("0")),
                    input.data.recipe.project_url.clone().ok_or(
                        Error::NoProjectUrlFoundInRecipe(input.data.recipe.name.clone()),
                    )?,
                    prisma::gate::UniqueWhereParam::IdEquals(input.gate),
                    encoded_recipe,
                    encoded_package_meta,
                    vec![],
                )
                .exec()
                .await?;

        component_from_database(component)
    }

    async fn import_component(
        &self,
        ctx: &Context<'_>,
        input: ComponentInput,
    ) -> Result<Component> {
        let db = &ctx.data_unchecked::<SharedState>().lock().await.prisma;
        let encoded_recipe = serde_json::to_value(&input.data.recipe)?;
        let encoded_package_meta = serde_json::to_value(&input.data.packages)?;

        let (name, version, revision, gate_id) = (
            input.data.recipe.name.clone(),
            input
                .data
                .recipe
                .version
                .clone()
                .ok_or(Error::NoVersionFoundInRecipe(
                    input.data.recipe.name.clone(),
                ))?,
            input
                .data
                .recipe
                .revision
                .clone()
                .unwrap_or(String::from("0")),
            input.gate,
        );

        let project_url =
            input
                .data
                .recipe
                .project_url
                .clone()
                .ok_or(Error::NoProjectUrlFoundInRecipe(
                    input.data.recipe.name.clone(),
                ))?;

        let mut optional_params: Vec<prisma::component::SetParam> = vec![];
        let mut update_params: Vec<prisma::component::SetParam> = vec![
            prisma::component::SetParam::SetRecipe(encoded_recipe.clone()),
            prisma::component::SetParam::SetPackages(encoded_package_meta.clone()),
            prisma::component::SetParam::SetProjectUrl(project_url.clone()),
        ];

        if let Some(anytia_id) = input.anitya_id {
            optional_params.push(prisma::component::SetParam::SetAnityaId(Some(
                anytia_id.clone(),
            )));
            update_params.push(prisma::component::SetParam::SetAnityaId(Some(anytia_id)));
        }

        if let Some(repology_id) = input.repology_id {
            optional_params.push(prisma::component::SetParam::SetRepologyId(Some(
                repology_id.clone(),
            )));
            update_params.push(prisma::component::SetParam::SetRepologyId(Some(
                repology_id,
            )));
        }

        let component = db
            .component()
            .upsert(
                prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                    name.clone(),
                    gate_id.clone(),
                    version.clone(),
                    revision.clone(),
                ),
                (
                    name,
                    version,
                    revision,
                    project_url,
                    prisma::gate::UniqueWhereParam::IdEquals(gate_id),
                    encoded_recipe,
                    encoded_package_meta,
                    optional_params.clone(),
                ),
                update_params,
            )
            .exec()
            .await?;

        component_from_database(component)
    }

    async fn upload_component_file(
        &self,
        ctx: &Context<'_>,
        component: ComponentsIdentifier,
        kind: ComponentFileKind,
        url: Option<String>,
        file: Option<Upload>,
    ) -> Result<Empty> {
        trace!("processing file upload for component {}", &component.name);

        let state = &ctx.data_unchecked::<SharedState>().lock().await;

        let db = &state.prisma;
        let fs = &state.fs_operator;

        trace!("fetching component from database");
        let component = db
            .component()
            .find_unique(
                prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                    component.name,
                    component.gate_id,
                    component.version,
                    component.revision,
                ),
            )
            .exec()
            .await?
            .ok_or(Error::NoComponentFound)?;

        trace!("component found proceeding with upload");
        if let Some(url) = url {
            let url: Url = url.parse()?;
            let filename = url.to_file_path().map_err(|_| async_graphql::Error {
                message: String::from("no filename in the url"),
                source: None,
                extensions: None,
            })?;
            let filename = filename
                .file_name()
                .ok_or(Error::String(String::from("no filename")))?
                .to_str()
                .ok_or(Error::String(String::from("no filename")))?;

            let tmp_name = format!(
                "trans/{}:{}:{}",
                kind,
                &component.name,
                uuid::Uuid::new_v4().to_string()
            );
            
            trace!("starting download of {} to {}", &url, &tmp_name);

            let mut writer = fs.writer_with(&tmp_name).buffer(8 * 1024 * 1024).await?;
            let response = reqwest::get(url).await?;
            let content = response.bytes().await?;
            tokio::io::copy(&mut content.to_vec().as_slice(), &mut writer).await?;
            writer.close().await?;
            
            let mut hasher = sha3::Sha3_256::new();
            hasher.update(fs.read(&tmp_name).await?);
            let result = hex::encode(hasher.finalize());
            
            let final_name = forge::ComponentFile {
                kind: kind.into(),
                component: component.name.clone(),
                name: filename.to_owned(),
                hash: result,
            };

            trace!("placing file on the final location {}", &final_name.to_string());
            // Only copy the file to the final destination if none exists there already
            if !fs.is_exist(&filename.to_string()).await? {
                fs.copy(&tmp_name, &final_name.to_string()).await?;
            }
            
            fs.delete(&tmp_name).await?;
            Ok(Empty { success: true })
        } else if let Some(upload) = file {
            let tmp_name = format!(
                "trans/{}:{}:{}",
                kind,
                &component.name,
                uuid::Uuid::new_v4().to_string()
            );
            
            trace!("starting upload of file to {}", &tmp_name);

            let mut writer = fs.writer_with(&tmp_name).buffer(8 * 1024 * 1024).await?;
            let upload = upload.value(&ctx)?;
            let filename = upload.filename.clone();
            tokio::io::copy(&mut tokio::fs::File::from(upload.content), &mut writer).await?;
            writer.close().await?;
            
            let mut hasher = sha3::Sha3_256::new();
            hasher.update(fs.read(&tmp_name).await?);
            let result = hex::encode(hasher.finalize());
            
            let final_name = forge::ComponentFile {
                kind: kind.into(),
                component: component.name.clone(),
                name: filename,
                hash: result,
            };

            trace!("placing file on the final location {}", &final_name.to_string());
            // Only copy the file to the final destination if none exists there already
            if !fs.is_exist(&final_name.to_string()).await? {
                fs.copy(&tmp_name, &final_name.to_string()).await?;
            }
            
            fs.delete(&tmp_name).await?;
            Ok(Empty { success: true })
        } else {
            Err(async_graphql::Error{
                message: String::from("neither file nor url provided"),
                source: None,
                extensions: None,
            })
        }
    }
}
