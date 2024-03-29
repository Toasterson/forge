use crate::prisma::{self, PrismaClient};
use crate::{Error, Result};
use component::Recipe;
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::Channel;
use forge::{ActivityObject, ComponentChangeKind, Event};
use tracing::{debug, error, instrument};

#[instrument(skip_all)]
pub async fn handle_message(
    deliver: Delivery,
    db: &PrismaClient,
    _channel: &Channel,
) -> Result<()> {
    let body = deliver.data;
    let envelope: Event = serde_json::from_slice(&body)?;
    match envelope {
        Event::Create(envelope) => {
            debug!("got create event: {:?}", envelope);
            match envelope.object {
                // We assume Create events are only sent when the webhook records the opening of a PR
                // All Jobs send updates to the ChangeRequest
                ActivityObject::ChangeRequest(change_request) => {
                    let db_cr = db
                        .change_request()
                        .create(vec![
                            //prisma::change_request::SetParam::SetId(uuid::Uuid::new_v4().to_string()),
                            prisma::change_request::SetParam::SetProcessing(true),
                            prisma::change_request::SetParam::SetExternalReference(Some(
                                change_request.external_ref.to_string(),
                            )),
                        ])
                        .exec()
                        .await?;
                    debug!(
                        "created change request with id: {} for reference: {}",
                        db_cr.id,
                        change_request.external_ref.to_string()
                    );
                    Ok(())
                }
                ActivityObject::Component { component, gate } => {
                    let recipe = &component.recipe;
                    db.component()
                        .create(
                            component.recipe.name.clone(),
                            recipe
                                .version
                                .clone()
                                .ok_or(Error::NoVersionFoundInRecipe(recipe.name.clone()))?,
                            recipe.revision.clone().unwrap_or(String::from("0")),
                            recipe
                                .project_url
                                .clone()
                                .ok_or(Error::NoProjectUrlFoundInRecipe(recipe.name.clone()))?,
                            prisma::gate::UniqueWhereParam::IdEquals(gate),
                            serde_json::to_value(&component.recipe)?,
                            serde_json::to_value(&component.package_meta)?,
                            vec![],
                        )
                        .exec()
                        .await?;
                    Ok(())
                }
                ActivityObject::Gate(gate) => {
                    db.gate()
                        .create(
                            gate.name,
                            gate.version,
                            gate.branch,
                            prisma::publisher::UniqueWhereParam::NameEquals(gate.publisher),
                            serde_json::to_value(&gate.default_transforms)?,
                            vec![],
                        )
                        .exec()
                        .await?;
                    Ok(())
                }
            }
        }
        Event::Update(envelope) => {
            debug!("got update event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(change_request) => {
                    // Take processing Component Changes that are processing and send them to the dedicated handler inbox
                    // Ones that have a defined change kind are to be upserted to the database as they are already processed externally
                    for component_change in change_request.changes {
                        // First we find the component this change relates to. May be none thus we assume it as optional
                        let mut component_where_args = vec![prisma::component::name::equals(
                            component_change.recipe.name.clone(),
                        )];
                        if let Some(metadata) = &component_change.recipe.metadata {
                            for item in &metadata.0 {
                                match item.name.as_str() {
                                    "anitya-id" => {
                                        component_where_args.push(
                                            prisma::component::anitya_id::equals(Some(
                                                item.value.clone(),
                                            )),
                                        );
                                    }
                                    "repology-id" => {
                                        component_where_args.push(
                                            prisma::component::repology_id::equals(Some(
                                                item.value.clone(),
                                            )),
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                        let component = db
                            .component()
                            .find_first(vec![prisma::component::WhereParam::Or(
                                component_where_args,
                            )])
                            .exec()
                            .await?;
                        match component_change.kind {
                            ComponentChangeKind::Processing => {
                                // If Processing is set it is assumed we need to check with the database
                                // if this component already exists or not
                                if let Some(component) = component {
                                    // Component exists record a modification
                                    let recipe: Recipe = serde_json::from_value(component.recipe)?;
                                    let _change = db.component_change().create(
                                        prisma::ComponentChangeKind::Updated,
                                        if let Some(diff) = &component_change.recipe_diff {
                                            serde_json::to_value(diff).unwrap_or(serde_json::Value::Null)
                                        } else {
                                            serde_json::Value::Null
                                        },
                                        serde_json::to_value(&component_change.recipe).unwrap_or(serde_json::Value::Null),
                                        component_change.recipe.version.ok_or(Error::NoVersionFoundInRecipe(component_change.component_ref.clone()))?,
                                        component_change.recipe.revision.unwrap_or(String::from("0")),
                                        prisma::change_request::UniqueWhereParam::IdEquals(change_request.id.clone()),
                                        vec![
                                            prisma::component_change::SetParam::ConnectComponent(prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                                                recipe.name.clone(),
                                                component.gate_id,
                                                recipe.version.ok_or(Error::NoVersionFoundInRecipe(component.name.clone()))?,
                                                recipe.revision.ok_or(Error::NoRevisionFoundInRecipe(component.name.clone()))?,
                                            ),),
                                            prisma::component_change::SetParam::ConnectChangeRequest(prisma::change_request::UniqueWhereParam::IdEquals(change_request.id.clone())),
                                        ],
                                    ).exec().await?;
                                } else {
                                    // record an addition
                                    let _change = db.component_change().create(
                                        prisma::ComponentChangeKind::Added,
                                        if let Some(diff) = &component_change.recipe_diff {
                                            serde_json::to_value(diff).unwrap_or(serde_json::Value::Null)
                                        } else {
                                            serde_json::Value::Null
                                        },
                                        serde_json::to_value(&component_change.recipe).unwrap_or(serde_json::Value::Null),
                                        component_change.recipe.version.ok_or(Error::NoVersionFoundInRecipe(component_change.component_ref.clone()))?,
                                        component_change.recipe.revision.unwrap_or(String::from("0")),
                                        prisma::change_request::UniqueWhereParam::IdEquals(change_request.id.clone()),
                                        vec![
                                            prisma::component_change::SetParam::ConnectChangeRequest(prisma::change_request::UniqueWhereParam::IdEquals(change_request.id.clone())),
                                        ],
                                    ).exec().await?;
                                }
                            }
                            kind => {
                                // if kind is known we assume we need to record that change.
                                let _change = db.component_change().create(
                                    match kind {
                                        ComponentChangeKind::Added => prisma::ComponentChangeKind::Added,
                                        ComponentChangeKind::Updated => prisma::ComponentChangeKind::Updated,
                                        ComponentChangeKind::Removed => prisma::ComponentChangeKind::Removed,
                                        ComponentChangeKind::Processing => panic!("component kind processing should already have been caught. If you see this talk to the person originally designing this as you have broken it"),
                                    },
                                    if let Some(diff) = &component_change.recipe_diff {
                                        serde_json::to_value(diff).unwrap_or(serde_json::Value::Null)
                                    } else {
                                        serde_json::Value::Null
                                    },
                                    serde_json::to_value(&component_change.recipe).unwrap_or(serde_json::Value::Null),
                                    component_change.recipe.version.ok_or(Error::NoVersionFoundInRecipe(component_change.component_ref.clone()))?,
                                    component_change.recipe.revision.unwrap_or(String::from("0")),
                                    prisma::change_request::UniqueWhereParam::IdEquals(change_request.id.clone()),
                                    vec![
                                        prisma::component_change::SetParam::ConnectChangeRequest(prisma::change_request::UniqueWhereParam::IdEquals(change_request.id.clone())),
                                    ],
                                ).exec().await?;
                            }
                        }
                    }
                    Ok(())
                }
                ActivityObject::Component { component, gate } => {
                    let recipe = &component.recipe;
                    db.component()
                        .update(
                            prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                                recipe.name.clone(),
                                gate,
                                recipe
                                    .version
                                    .clone()
                                    .ok_or(Error::NoVersionFoundInRecipe(recipe.name.clone()))?
                                    .clone(),
                                recipe
                                    .revision
                                    .clone()
                                    .ok_or(Error::NoRevisionFoundInRecipe(recipe.name.clone()))?
                                    .clone(),
                            ),
                            vec![
                                prisma::component::SetParam::SetProjectUrl(
                                    recipe.project_url.clone().ok_or(
                                        Error::NoProjectUrlFoundInRecipe(recipe.name.clone()),
                                    )?,
                                ),
                                prisma::component::SetParam::SetRecipe(serde_json::to_value(
                                    &component.recipe,
                                )?),
                                prisma::component::SetParam::SetPackages(serde_json::to_value(
                                    &component.package_meta,
                                )?),
                            ],
                        )
                        .exec()
                        .await?;
                    Ok(())
                }
                ActivityObject::Gate(gate) => {
                    db.gate()
                        .update(
                            prisma::gate::UniqueWhereParam::IdEquals(
                                gate.id.ok_or(Error::NoIdFoundINGate(gate.name.clone()))?,
                            ),
                            vec![
                                prisma::gate::SetParam::SetVersion(gate.version),
                                prisma::gate::SetParam::SetName(gate.name),
                                prisma::gate::SetParam::SetBranch(gate.branch),
                                prisma::gate::SetParam::ConnectPublisher(
                                    prisma::publisher::UniqueWhereParam::NameEquals(gate.publisher),
                                ),
                                prisma::gate::SetParam::SetTransforms(serde_json::to_value(
                                    &gate.default_transforms,
                                )?),
                            ],
                        )
                        .exec()
                        .await?;
                    Ok(())
                }
            }
        }
        Event::Delete(envelope) => {
            debug!("got delete event: {:?}", envelope);
            match envelope.object {
                ActivityObject::ChangeRequest(_) => {
                    error!("We do not support deleting change_requests, please delete manually in the database");
                    Ok(())
                }
                ActivityObject::Component { .. } => {
                    error!("We do not support deleting components, please delete manually in the database");
                    Ok(())
                }
                ActivityObject::Gate(_) => {
                    error!(
                        "We do not support deleting gates, please delete manually in the database"
                    );
                    Ok(())
                }
            }
        }
    }
}
