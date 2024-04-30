use crate::{prisma, Error, Result, SharedState};
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::post;
use axum::{Json, Router};
use component::{PackageMeta, Recipe};
use serde::{Deserialize, Serialize};
use sha3::Digest;
use tracing::trace;
use url::Url;
use utoipa::ToSchema;

pub fn get_router() -> Router<SharedState> {
    Router::new()
        .route("/list", post(list_components))
        .route("/get", post(get_component))
        .route("/", post(create_component))
        .route("/import", post(import_component))
        .route("/upload/:kind", post(upload_to_component))
        .layer(DefaultBodyLimit::max(629145600))
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GetComponentRequest {
    name: String,
    version: String,
    revision: String,
    gate_id: String,
}
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct Component {
    pub name: String,
    pub version: String,
    pub revision: String,
    pub anitya_id: Option<String>,
    pub repology_id: Option<String>,
    pub project_url: String,
    pub gate_id: String,
    pub recipe: Recipe,
    pub packages: PackageMeta,
}

pub fn component_from_database(component: prisma::component::Data) -> Result<Component> {
    let r = Component {
        name: component.name,
        version: component.version,
        gate_id: component.gate_id.to_string(),
        revision: component.revision,
        anitya_id: component.anitya_id.clone(),
        repology_id: component.repology_id.clone(),
        project_url: component.project_url,
        recipe: serde_json::from_value(component.recipe)?,
        packages: serde_json::from_value(component.packages)?,
    };
    Ok(r)
}

#[utoipa::path(
    post,
    path = "/api/v1/components/get",
    request_body = GetComponentRequest,
    responses (
        (status = 200, description = "Successfully got the Component", body = Component),
        (status = 404, description = "Component not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn get_component(
    State(state): State<SharedState>,
    Json(request): Json<GetComponentRequest>,
) -> Result<Json<Component>> {
    let component = state
        .lock()
        .await
        .prisma
        .component()
        .find_unique(
            prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                request.name,
                request.version,
                request.revision,
                request.gate_id,
            ),
        )
        .exec()
        .await?;

    if let Some(component) = component {
        Ok(Json(component_from_database(component)?))
    } else {
        Err(Error::NoComponentFound)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ListComponentRequest {
    name: String,
    version: Option<String>,
    revision: Option<String>,
    gate_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/components/list",
    request_body = ListComponentRequest,
    responses (
        (status = 200, description = "Successfully retrieved component info", body = Vec<Component>),
        (status = 404, description = "Component not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn list_components(
    State(state): State<SharedState>,
    Json(request): Json<ListComponentRequest>,
) -> Result<Json<Vec<Component>>> {
    let mut filter = vec![prisma::component::name::equals(request.name)];

    if let Some(version) = request.version {
        filter.push(prisma::component::version::equals(version))
    }

    if let Some(revision) = request.revision {
        filter.push(prisma::component::revision::equals(revision))
    }

    if let Some(gate_id) = request.gate_id {
        filter.push(prisma::component::gate_id::equals(gate_id))
    }

    let components = state
        .lock()
        .await
        .prisma
        .component()
        .find_many(filter)
        .exec()
        .await?;

    let components = components
        .into_iter()
        .filter_map(|component| component_from_database(component).ok())
        .collect();

    Ok(Json(components))
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ComponentInput {
    pub recipe: Recipe,
    pub packages: PackageMeta,
    pub anitya_id: Option<String>,
    pub repology_id: Option<String>,
    pub gate: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ComponentIdentifier {
    pub name: String,
    pub version: String,
    pub revision: String,
    pub gate_id: String,
}

#[utoipa::path(
    post,
    path = "/api/v1/components/",
    request_body = ComponentInput,
    responses (
        (status = 200, description = "Successfully retrieved component info", body = Component),
        (status = 401, description = "Unauthorized to access the API", body = ApiError, example = json!(crate::ApiError::Unauthorized)),
        (status = 404, description = "Component not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn create_component(
    State(state): State<SharedState>,
    Json(request): Json<ComponentInput>,
) -> Result<Json<Component>> {
    let encoded_recipe = serde_json::to_value(&request.recipe)?;
    let encoded_package_meta = serde_json::to_value(&request.packages)?;

    let component = state
        .lock()
        .await
        .prisma
        .component()
        .create(
            request.recipe.name.clone(),
            request
                .recipe
                .version
                .clone()
                .ok_or(Error::NoVersionFoundInRecipe(request.recipe.name.clone()))?,
            request.recipe.revision.clone().unwrap_or(String::from("0")),
            request
                .recipe
                .project_url
                .clone()
                .ok_or(Error::NoProjectUrlFoundInRecipe(
                    request.recipe.name.clone(),
                ))?,
            prisma::gate::UniqueWhereParam::IdEquals(request.gate),
            encoded_recipe,
            encoded_package_meta,
            vec![],
        )
        .exec()
        .await?;

    Ok(Json(component_from_database(component)?))
}

#[utoipa::path(
    post,
    path = "/api/v1/components/import",
    request_body = ComponentInput,
    responses (
        (status = 200, description = "Successfully retrieved component info", body = Component),
        (status = 401, description = "Unauthorized to access the API", body = ApiError, example = json!(crate::ApiError::Unauthorized)),
        (status = 404, description = "Component not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    )
)]
async fn import_component(
    State(state): State<SharedState>,
    Json(request): Json<ComponentInput>,
) -> Result<Json<Component>> {
    let encoded_recipe = serde_json::to_value(&request.recipe)?;
    let encoded_package_meta = serde_json::to_value(&request.packages)?;

    let (name, version, revision, gate_id) = (
        request.recipe.name.clone(),
        request
            .recipe
            .version
            .clone()
            .ok_or(Error::NoVersionFoundInRecipe(request.recipe.name.clone()))?,
        request.recipe.revision.clone().unwrap_or(String::from("0")),
        request.gate,
    );

    let project_url =
        request
            .recipe
            .project_url
            .clone()
            .ok_or(Error::NoProjectUrlFoundInRecipe(
                request.recipe.name.clone(),
            ))?;

    let mut optional_params: Vec<prisma::component::SetParam> = vec![];
    let mut update_params: Vec<prisma::component::SetParam> = vec![
        prisma::component::SetParam::SetRecipe(encoded_recipe.clone()),
        prisma::component::SetParam::SetPackages(encoded_package_meta.clone()),
        prisma::component::SetParam::SetProjectUrl(project_url.clone()),
    ];

    if let Some(anytia_id) = request.anitya_id {
        optional_params.push(prisma::component::SetParam::SetAnityaId(Some(
            anytia_id.clone(),
        )));
        update_params.push(prisma::component::SetParam::SetAnityaId(Some(anytia_id)));
    }

    if let Some(repology_id) = request.repology_id {
        optional_params.push(prisma::component::SetParam::SetRepologyId(Some(
            repology_id.clone(),
        )));
        update_params.push(prisma::component::SetParam::SetRepologyId(Some(
            repology_id,
        )));
    }

    let component = state
        .lock()
        .await
        .prisma
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

    Ok(Json(component_from_database(component)?))
}

#[derive(ToSchema, Debug)]
pub struct Upload {
    pub identifier: ComponentIdentifier,
    #[schema(value_type = String, format = Binary)]
    pub file: Option<Vec<u8>>,
    pub url: Option<Url>,
}

#[utoipa::path(
    post,
    path = "/api/v1/components/upload/{kind}",
    request_body(content = Upload, description = "Multipart file or url", content_type = "multipart/form-data"),
    responses (
        (status = 200, description = "Upload successful"),
        (status = 401, description = "Unauthorized to access the API", body = ApiError, example = json!(crate::ApiError::Unauthorized)),
        (status = 404, description = "Component not found", body = ApiError, example = json!(crate::ApiError::NotFound(String::from("id = 1"))))
    ),
    params(
        ("kind" = String, Path, description = "Kind of file to upload"),
    )
)]
async fn upload_to_component(
    State(state): State<SharedState>,
    Path(kind): Path<String>,
    mut multipart: axum::extract::Multipart,
) -> Result<()> {
    let component_ident: ComponentIdentifier = if let Some(field) = multipart.next_field().await? {
        let buf = field.bytes().await?.to_vec();
        serde_json::from_slice(&buf)?
    } else {
        return Err(Error::InvalidMultipartRequest);
    };

    let mut files: Vec<(String, Bytes)> = vec![];
    let mut urls: Vec<String> = vec![];

    while let Some(field) = multipart.next_field().await? {
        if let Some(file_name) = field.file_name() {
            files.push((file_name.to_owned(), field.bytes().await?));
        } else {
            urls.push(field.text().await?);
        }
    }

    trace!(
        "processing file upload for component {}",
        &component_ident.name
    );

    trace!("fetching component from database");
    let component = state
        .lock()
        .await
        .prisma
        .component()
        .find_unique(
            prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                component_ident.name,
                component_ident.gate_id,
                component_ident.version,
                component_ident.revision,
            ),
        )
        .exec()
        .await?
        .ok_or(Error::NoComponentFound)?;

    trace!("component found proceeding with upload");
    for url in urls {
        let url: Url = url.parse()?;
        let filename = url
            .to_file_path()
            .map_err(|_| Error::InvalidMultipartRequest)?;
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

        let mut writer = state
            .lock()
            .await
            .fs_operator
            .writer_with(&tmp_name)
            .buffer(8 * 1024 * 1024)
            .await?;
        let response = reqwest::get(url).await?;
        let content = response.bytes().await?;
        tokio::io::copy(&mut content.to_vec().as_slice(), &mut writer).await?;
        writer.close().await?;

        let mut hasher = sha3::Sha3_256::new();
        hasher.update(state.lock().await.fs_operator.read(&tmp_name).await?);
        let result = hex::encode(hasher.finalize());

        let final_name = forge::ComponentFile {
            kind: kind.parse()?,
            component: component.name.clone(),
            name: filename.to_owned(),
            hash: result,
        };

        trace!(
            "placing file on the final location {}",
            &final_name.to_string()
        );
        // Only copy the file to the final destination if none exists there already
        if !state
            .lock()
            .await
            .fs_operator
            .is_exist(&filename.to_string())
            .await?
        {
            state
                .lock()
                .await
                .fs_operator
                .copy(&tmp_name, &final_name.to_string())
                .await?;
        }

        state.lock().await.fs_operator.delete(&tmp_name).await?;
    }

    for file in files {
        let tmp_name = format!(
            "trans/{}:{}:{}",
            kind,
            &component.name,
            uuid::Uuid::new_v4().to_string()
        );

        trace!("starting upload of file to {}", &tmp_name);

        let mut writer = state
            .lock()
            .await
            .fs_operator
            .writer_with(&tmp_name)
            .buffer(8 * 1024 * 1024)
            .await?;
        let filename = file.0;
        tokio::io::copy(&mut file.1.to_vec().as_slice(), &mut writer).await?;
        writer.close().await?;

        let mut hasher = sha3::Sha3_256::new();
        hasher.update(state.lock().await.fs_operator.read(&tmp_name).await?);
        let result = hex::encode(hasher.finalize());

        let final_name = forge::ComponentFile {
            kind: kind.parse()?,
            component: component.name.clone(),
            name: filename,
            hash: result,
        };

        trace!(
            "placing file on the final location {}",
            &final_name.to_string()
        );
        // Only copy the file to the final destination if none exists there already
        if !state
            .lock()
            .await
            .fs_operator
            .is_exist(&final_name.to_string())
            .await?
        {
            state
                .lock()
                .await
                .fs_operator
                .copy(&tmp_name, &final_name.to_string())
                .await?;
        }

        state.lock().await.fs_operator.delete(&tmp_name).await?;
    }

    Ok(())
}
