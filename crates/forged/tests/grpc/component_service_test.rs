mod common;
use common::{fixtures::TestFixtures, TestContext};
use forged::transport::grpc::proto::{
    component_service_server::ComponentService, ActorRef, ComponentId, CreateComponentRequest,
    GateId, GetComponentRequest, UpdateComponentRequest,
};
use forged::transport::ComponentServiceImpl;

fn actor_ref(id: String) -> ActorRef {
    ActorRef {
        id,
        kind: "user".to_string(),
    }
}

#[tokio::test]
async fn test_create_component() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    let request = tonic::Request::new(CreateComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
        name: "test-component".to_string(),
        recipe_kdl: "component { name = \"test-component\" }".to_string(),
    });

    let response = component_service
        .create_component(request)
        .await
        .expect("Failed to create component");

    let component_info = response.into_inner().component.unwrap();

    assert_eq!(component_info.name, "test-component");
    assert_eq!(component_info.gate_id, gate.id);
    assert!(!component_info.id.is_empty());
}

#[tokio::test]
async fn test_get_component() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    let request = tonic::Request::new(GetComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
    });

    let response = component_service
        .get_component(request)
        .await
        .expect("Failed to get component");

    let component_info = response.into_inner().component.unwrap();

    assert_eq!(component_info.id, component.id);
    assert_eq!(component_info.name, "test-component");
    assert_eq!(component_info.gate_id, gate.id);
}

#[tokio::test]
async fn test_get_nonexistent_component() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(GetComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: "nonexistent".to_string(),
        }),
    });

    let result = component_service.get_component(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_update_component() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;
    let component = TestFixtures::component(&ctx, &owner, &gate, "test-component").await;

    let request = tonic::Request::new(UpdateComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
        recipe_kdl: "component { version = \"2.0\" }".to_string(),
    });

    let response = component_service
        .update_component(request)
        .await
        .expect("Failed to update component");

    let component_info = response.into_inner().component.unwrap();

    assert_eq!(component_info.id, component.id);
    assert_eq!(component_info.recipe_kdl, "component { version = \"2.0\" }");
}

#[tokio::test]
async fn test_create_multiple_components_in_gate() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Create multiple components
    let mut component_ids = Vec::new();
    for i in 0..5 {
        let request = tonic::Request::new(CreateComponentRequest {
            actor: Some(actor_ref(owner.id.clone())),
            gate_id: Some(GateId { id: gate.id.clone() }),
            name: format!("component-{}", i),
            recipe_kdl: format!("component {{ name = \"component-{}\" }}", i),
        });

        let response = component_service
            .create_component(request)
            .await
            .expect("Failed to create component");

        let component_info = response.into_inner().component.unwrap();
        component_ids.push(component_info.id);
    }

    assert_eq!(component_ids.len(), 5);

    // All IDs should be unique
    let unique_ids: std::collections::HashSet<_> = component_ids.iter().collect();
    assert_eq!(unique_ids.len(), 5);
}

#[tokio::test]
async fn test_missing_actor() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    let request = tonic::Request::new(CreateComponentRequest {
        actor: None, // Missing actor
        gate_id: Some(GateId { id: gate.id.clone() }),
        name: "test-component".to_string(),
        recipe_kdl: "component { }".to_string(),
    });

    let result = component_service.create_component(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_missing_gate_id() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(CreateComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: None, // Missing gate_id
        name: "test-component".to_string(),
        recipe_kdl: "component { }".to_string(),
    });

    let result = component_service.create_component(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn test_component_lifecycle() {
    let ctx = TestContext::new().await;
    let component_service = ComponentServiceImpl::new(
        ctx.app_state.component_repo.clone(),
        ctx.app_state.source_archive_repo.clone(),
    );

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Create
    let create_request = tonic::Request::new(CreateComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
        name: "lifecycle-component".to_string(),
        recipe_kdl: "component { version = \"1.0\" }".to_string(),
    });

    let create_response = component_service
        .create_component(create_request)
        .await
        .expect("Failed to create component");

    let component = create_response.into_inner().component.unwrap();

    // Read
    let get_request = tonic::Request::new(GetComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
    });

    let get_response = component_service
        .get_component(get_request)
        .await
        .expect("Failed to get component");

    let fetched_component = get_response.into_inner().component.unwrap();
    assert_eq!(fetched_component.id, component.id);
    assert_eq!(fetched_component.recipe_kdl, "component { version = \"1.0\" }");

    // Update
    let update_request = tonic::Request::new(UpdateComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
        recipe_kdl: "component { version = \"2.0\" }".to_string(),
    });

    let update_response = component_service
        .update_component(update_request)
        .await
        .expect("Failed to update component");

    let updated_component = update_response.into_inner().component.unwrap();
    assert_eq!(updated_component.recipe_kdl, "component { version = \"2.0\" }");

    // Verify update persisted
    let verify_request = tonic::Request::new(GetComponentRequest {
        actor: Some(actor_ref(owner.id.clone())),
        component_id: Some(ComponentId {
            id: component.id.clone(),
        }),
    });

    let verify_response = component_service
        .get_component(verify_request)
        .await
        .expect("Failed to verify component");

    let verified_component = verify_response.into_inner().component.unwrap();
    assert_eq!(verified_component.recipe_kdl, "component { version = \"2.0\" }");
}
