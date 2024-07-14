use crate::prisma::read_filters::{BoolFilter, StringFilter};
use crate::prisma::{self, PrismaClient};
use crate::{Error, Result};
use component::Recipe;
use deadpool_lapin::lapin::message::Delivery;
use deadpool_lapin::lapin::Channel;
use diff::Diff;
use forge::{ActivityObject, ChangeRequestState, Event, JobReport, JobReportData};
use tracing::{debug, error, info, instrument};
use crate::component_helpers::find_latest_component_in_set;

#[instrument(skip_all)]
pub async fn handle_message(
    deliver: Delivery,
    routing_key: &str,
    db: &PrismaClient,
    _channel: &Channel,
) -> Result<()> {
    let body = deliver.data;
    match routing_key {
        "forged.jobreport" => {
            let report: JobReport = serde_json::from_slice(&body)?;
            match report {
                JobReport::Success(data) => match data {
                    JobReportData::GetRecipes {
                        gate_id,
                        change_request_id,
                        recipes,
                    } => {
                        debug!("Processing Job report from worker");
                        for (component_ref, recipe, package_meta, patches) in recipes {
                            debug!("Processing component {component_ref}");
                            let name = recipe.name.clone();
                            let version = recipe
                                .version
                                .clone()
                                .ok_or(Error::NoVersionFoundInRecipe(recipe.name.clone()))?;
                            let revision = recipe.revision.clone().unwrap_or("0".to_string());

                            // Check first if we have the gate we are trying to record a change for
                            if !db
                                .gate()
                                .find_unique(prisma::gate::UniqueWhereParam::IdEquals(
                                    gate_id.to_string(),
                                ))
                                .exec()
                                .await
                                .is_ok()
                            {
                                info!("No gate record found for requested change to component {} skipping", name);
                                continue;
                            }

                            let mut set_params = vec![];
                            let components =
                                db.component()
                                    .find_many(
                                        vec![
                                            prisma::component::WhereParam::Name(StringFilter::Equals(name.clone())),
                                            prisma::component::WhereParam::GateId(StringFilter::Equals(gate_id.to_string())),
                                        ]
                                    ).exec().await?;

                            let change_kind = if !components.is_empty() {
                                debug!("Component has changed");
                                prisma::ComponentChangeKind::Updated
                            } else {
                                debug!("new Component was added");
                                prisma::ComponentChangeKind::Added
                            };

                            let recipe_value = serde_json::to_value(&recipe)?;

                            set_params.push(prisma::component_change::SetParam::ConnectGate(
                                prisma::gate::UniqueWhereParam::IdEquals(gate_id.to_string()),
                            ));

                            let recipe_diff = if !components.is_empty() {
                                let component = find_latest_component_in_set(components)?;
                                set_params.push(prisma::component_change::SetParam::ConnectComponent(
                                    prisma::component::UniqueWhereParam::NameGateIdVersionRevisionEquals(
                                        component.name,
                                        component.gate_id,
                                        component.version,
                                        component.revision,
                                    ),
                                ));

                                let existing_recipe: Recipe =
                                    serde_json::from_value(component.recipe)?;
                                let recipe_diff = existing_recipe.diff(&recipe);

                                serde_json::to_value(&recipe_diff)?
                            } else {
                                serde_json::Value::Null
                            };

                            if let Some(package_meta) = package_meta {
                                let package_meta_value = serde_json::to_value(&package_meta)?;
                                set_params.push(prisma::component_change::SetParam::SetPackageMeta(
                                    package_meta_value
                                ));
                            }

                            let patch_value = serde_json::to_value(&patches)?;

                            debug!("Writing component change to database");
                            // Try to find an existing change for this component inside this CR (under the assumption a previous update already worked.)
                            let possible_existing_component_change = db
                                .component_change()
                                .find_first(vec![prisma::component_change::WhereParam::And(vec![
                                    prisma::component_change::WhereParam::ChangeRequestId(
                                        StringFilter::Equals(change_request_id.to_string()),
                                    ),
                                    prisma::component_change::WhereParam::Name(
                                        StringFilter::Equals(name.clone()),
                                    ),
                                ])])
                                .exec()
                                .await?;

                            if let Some(db_component_change) = possible_existing_component_change {
                                debug!("Found existing change updating it");
                                set_params
                                    .push(prisma::component_change::SetParam::SetKind(change_kind));
                                set_params
                                    .push(prisma::component_change::SetParam::SetDiff(recipe_diff));
                                set_params.push(prisma::component_change::SetParam::SetRecipe(
                                    recipe_value,
                                ));
                                set_params
                                    .push(prisma::component_change::SetParam::SetVersion(version));
                                set_params.push(prisma::component_change::SetParam::SetRevision(
                                    revision,
                                ));
                                set_params.push(prisma::component_change::SetParam::SetPatches(
                                    patch_value,
                                ));

                                db.component_change()
                                    .update(
                                        prisma::component_change::UniqueWhereParam::IdEquals(
                                            db_component_change.id,
                                        ),
                                        set_params,
                                    )
                                    .exec()
                                    .await?;
                            } else {
                                debug!("Creating new change record");
                                db.component_change()
                                    .create(
                                        change_kind,
                                        recipe_diff,
                                        name.clone(),
                                        recipe_value,
                                        version,
                                        revision,
                                        patch_value,
                                        prisma::change_request::UniqueWhereParam::IdEquals(
                                            change_request_id.clone(),
                                        ),
                                        set_params,
                                    )
                                    .exec()
                                    .await?;
                            }

                            db.change_request()
                                .update(
                                    prisma::change_request::UniqueWhereParam::IdEquals(
                                        change_request_id.clone(),
                                    ),
                                    vec![prisma::change_request::SetParam::SetProcessing(false)],
                                )
                                .exec()
                                .await?;
                        }

                        Ok(())
                    }
                },
                JobReport::Failure {
                    error,
                    object,
                    kind,
                } => {
                    error!("Job reported an error {error} while processing {kind} for {object}");

                    Ok(())
                }
            }
        }
        "forged.event" => {
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
                                .create(
                                    change_request.id,
                                    vec![
                                        prisma::change_request::SetParam::SetProcessing(true),
                                        prisma::change_request::SetParam::SetExternalReference(
                                            Some(change_request.external_ref.to_string()),
                                        ),
                                    ],
                                )
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
                                    recipe.version.clone().ok_or(Error::NoVersionFoundInRecipe(
                                        recipe.name.clone(),
                                    ))?,
                                    recipe.revision.clone().unwrap_or(String::from("0")),
                                    recipe.project_url.clone().ok_or(
                                        Error::NoProjectUrlFoundInRecipe(recipe.name.clone()),
                                    )?,
                                    prisma::gate::UniqueWhereParam::IdEquals(gate),
                                    serde_json::to_value(&component.recipe)?,
                                    serde_json::Value::Null, //TODO Find solution to transport patches over the wire
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

                            let db_change_request = db
                                .change_request()
                                .upsert(
                                    prisma::change_request::UniqueWhereParam::IdEquals(
                                        change_request.id.clone(),
                                    ),
                                    (
                                        change_request.id,
                                        vec![
                                            prisma::change_request::SetParam::SetProcessing(true),
                                            prisma::change_request::SetParam::SetExternalReference(
                                                Some(change_request.external_ref.to_string()),
                                            ),
                                            prisma::change_request::SetParam::SetState(
                                                match change_request.state {
                                                    ChangeRequestState::Open => {
                                                        prisma::ChangeRequestState::Open
                                                    }
                                                    ChangeRequestState::Draft => {
                                                        prisma::ChangeRequestState::Draft
                                                    }
                                                    ChangeRequestState::Closed => {
                                                        prisma::ChangeRequestState::Closed
                                                    }
                                                    ChangeRequestState::Applied => {
                                                        prisma::ChangeRequestState::Applied
                                                    }
                                                },
                                            ),
                                        ],
                                    ),
                                    vec![],
                                )
                                .with(
                                    prisma::change_request::component_changes::fetch(vec![
                                        prisma::component_change::WhereParam::Applied(
                                            BoolFilter::Equals(false)
                                        )
                                    ])
                                )
                                .exec()
                                .await?;
                            debug!(
                                "created/updated change request with id: {} for reference: {}",
                                db_change_request.id,
                                change_request.external_ref.to_string()
                            );

                            match db_change_request.state {
                                prisma::ChangeRequestState::Applied => {
                                    db._transaction().run::<crate::Error, _, _, _>(|db| async move {
                                        if let Some(changes) = db_change_request.component_changes {
                                            for change in changes {
                                                let recipe: Recipe = serde_json::from_value(change.recipe.clone())?;
                                                let name = recipe.name.clone();
                                                let version = recipe
                                                    .version
                                                    .clone()
                                                    .ok_or(Error::NoVersionFoundInRecipe(recipe.name.clone()))?;
                                                let revision = recipe.revision.clone().unwrap_or("0".to_string());
                                                let mut component_set_params = vec![];
                                                if let Some(metadata) = &recipe.metadata {
                                                    for item in &metadata.0 {
                                                        match item.name.as_str() {
                                                            "anitya-id" => {
                                                                component_set_params.push(
                                                                    prisma::component::SetParam::SetAnityaId(Some(item.value.clone()))
                                                                );
                                                            }
                                                            "repology-id" => {
                                                                component_set_params.push(
                                                                    prisma::component::SetParam::SetRepologyId(Some(item.value.clone()))
                                                                );
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                }

                                                db.component_change().update(
                                                    prisma::component_change::UniqueWhereParam::IdEquals(change.id),
                                                    vec![
                                                        prisma::component_change::SetParam::SetApplied(true)
                                                    ]
                                                ).exec().await?;

                                                db.component().create(
                                                    name.clone(),
                                                    version,
                                                    revision,
                                                    recipe.project_url.ok_or(
                                                        Error::NoProjectUrlFoundInRecipe(name.clone())
                                                    )?,
                                                    prisma::gate::UniqueWhereParam::IdEquals(change.gate_id.unwrap()),
                                                    change.recipe,
                                                    change.patches,
                                                    change.package_meta,
                                                    component_set_params,
                                                ).exec().await?;
                                            }
                                        }
                                        Ok(())
                                    }).await?;
                                }
                                _ => {}
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
                                            prisma::publisher::UniqueWhereParam::NameEquals(
                                                gate.publisher,
                                            ),
                                        ),
                                        prisma::gate::SetParam::SetTransforms(
                                            serde_json::to_value(&gate.default_transforms)?,
                                        ),
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
        unknown => {
            error!("unknown routing key {unknown}");
            Ok(())
        }
    }
}
