mod common;
use common::{fixtures::TestFixtures, TestContext};
use forged::transport::grpc::proto::{
    gate_service_server::GateService, ActorRef, AddMemberRequest, CreateGateRequest,
    GateId, GetGateRequest, ListComponentsRequest, ListGatesRequest, ListMembersRequest,
    RemoveMemberRequest, UpdateGateRequest,
};
use forged::transport::GateServiceImpl;

fn actor_ref(id: String) -> ActorRef {
    ActorRef {
        id,
        kind: "user".to_string(),
    }
}

#[tokio::test]
async fn test_create_gate() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(CreateGateRequest {
        actor: Some(actor_ref(owner.id.clone())),
        name: "test-gate".to_string(),
        gate_kdl: "gate { name = \"test-gate\" }".to_string(),
    });

    let response = gate_service
        .create_gate(request)
        .await
        .expect("Failed to create gate");

    let gate_info = response.into_inner().gate.unwrap();

    assert_eq!(gate_info.name, "test-gate");
    assert_eq!(gate_info.owner_id, owner.id);
    assert!(!gate_info.id.is_empty());
}

#[tokio::test]
async fn test_get_gate() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    let request = tonic::Request::new(GetGateRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
    });

    let response = gate_service
        .get_gate(request)
        .await
        .expect("Failed to get gate");

    let gate_info = response.into_inner().gate.unwrap();

    assert_eq!(gate_info.id, gate.id);
    assert_eq!(gate_info.name, "test-gate");
}

#[tokio::test]
async fn test_get_nonexistent_gate() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;

    let request = tonic::Request::new(GetGateRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId {
            id: "nonexistent".to_string(),
        }),
    });

    let result = gate_service.get_gate(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn test_update_gate() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    let request = tonic::Request::new(UpdateGateRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
        gate_kdl: "gate { name = \"updated-gate\" }".to_string(),
    });

    let response = gate_service
        .update_gate(request)
        .await
        .expect("Failed to update gate");

    let gate_info = response.into_inner().gate.unwrap();

    assert_eq!(gate_info.id, gate.id);
    assert_eq!(gate_info.gate_kdl, "gate { name = \"updated-gate\" }");
}

#[tokio::test]
async fn test_add_member() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let member = TestFixtures::actor(&ctx, "member").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    let request = tonic::Request::new(AddMemberRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
        member_actor_id: member.id.clone(),
        roles: vec!["developer".to_string()],
        permissions: vec!["gate_read".to_string(), "component_read".to_string()],
    });

    let response = gate_service
        .add_member(request)
        .await
        .expect("Failed to add member");

    let member_info = response.into_inner().member.unwrap();

    assert_eq!(member_info.gate_id, gate.id);
    assert_eq!(member_info.actor_id, member.id);
    assert_eq!(member_info.roles, vec!["developer"]);
    assert_eq!(
        member_info.permissions,
        vec!["gate_read", "component_read"]
    );
}

#[tokio::test]
async fn test_list_members() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Add multiple members
    for i in 0..3 {
        let member = TestFixtures::actor(&ctx, &format!("member{}", i)).await;
        ctx.app_state
            .gate_manager
            .add_member(
                &owner.id,
                &gate.id,
                &member.id,
                vec![format!("role{}", i)],
                vec!["gate_read".to_string()],
            )
            .await
            .expect("Failed to add member");
    }

    let request = tonic::Request::new(ListMembersRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
    });

    let response = gate_service
        .list_members(request)
        .await
        .expect("Failed to list members");

    let members = response.into_inner().members;

    assert_eq!(members.len(), 3);
}

#[tokio::test]
async fn test_remove_member() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let member = TestFixtures::actor(&ctx, "member").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Add member
    ctx.app_state
        .gate_manager
        .add_member(
            &owner.id,
            &gate.id,
            &member.id,
            vec!["developer".to_string()],
            vec!["gate_read".to_string()],
        )
        .await
        .expect("Failed to add member");

    // Remove member
    let request = tonic::Request::new(RemoveMemberRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
        member_actor_id: member.id.clone(),
    });

    let response = gate_service
        .remove_member(request)
        .await
        .expect("Failed to remove member");

    assert!(response.into_inner().success);

    // Verify member was removed
    let list_request = tonic::Request::new(ListMembersRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
    });

    let list_response = gate_service
        .list_members(list_request)
        .await
        .expect("Failed to list members");

    let members = list_response.into_inner().members;
    assert_eq!(members.len(), 0);
}

#[tokio::test]
async fn test_list_components() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;
    let gate = TestFixtures::gate(&ctx, &owner, "test-gate").await;

    // Create components
    for i in 0..5 {
        TestFixtures::component(&ctx, &owner, &gate, &format!("component-{}", i)).await;
    }

    let request = tonic::Request::new(ListComponentsRequest {
        actor: Some(actor_ref(owner.id.clone())),
        gate_id: Some(GateId { id: gate.id.clone() }),
    });

    let response = gate_service
        .list_components(request)
        .await
        .expect("Failed to list components");

    let components = response.into_inner().components;

    assert_eq!(components.len(), 5);
}

#[tokio::test]
async fn test_list_gates_by_owner() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner = TestFixtures::actor(&ctx, "owner").await;

    // Create multiple gates
    for i in 0..3 {
        TestFixtures::gate(&ctx, &owner, &format!("gate-{}", i)).await;
    }

    let request = tonic::Request::new(ListGatesRequest {
        actor: Some(actor_ref(owner.id.clone())),
        owner_id: Some(owner.id.clone()),
    });

    let response = gate_service
        .list_gates(request)
        .await
        .expect("Failed to list gates");

    let gates = response.into_inner().gates;

    assert_eq!(gates.len(), 3);
}

#[tokio::test]
async fn test_list_all_gates() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let owner1 = TestFixtures::actor(&ctx, "owner1").await;
    let owner2 = TestFixtures::actor(&ctx, "owner2").await;

    // Create gates for different owners
    TestFixtures::gate(&ctx, &owner1, "gate-1").await;
    TestFixtures::gate(&ctx, &owner1, "gate-2").await;
    TestFixtures::gate(&ctx, &owner2, "gate-3").await;

    let request = tonic::Request::new(ListGatesRequest {
        actor: Some(actor_ref(owner1.id.clone())),
        owner_id: None, // List all
    });

    let response = gate_service
        .list_gates(request)
        .await
        .expect("Failed to list gates");

    let gates = response.into_inner().gates;

    assert!(gates.len() >= 3);
}

#[tokio::test]
async fn test_missing_actor() {
    let ctx = TestContext::new().await;
    let gate_service =
        GateServiceImpl::new(ctx.app_state.gate_repo.clone(), ctx.app_state.component_repo.clone());

    let request = tonic::Request::new(CreateGateRequest {
        actor: None, // Missing actor
        name: "test-gate".to_string(),
        gate_kdl: "gate { }".to_string(),
    });

    let result = gate_service.create_gate(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
