mod common;
use common::TestContext;
use forged::transport::grpc::proto::{auth_service_server::AuthService, AuthenticateRequest};
use forged::transport::AuthServiceImpl;

#[tokio::test]
async fn test_authenticate_creates_actor() {
    let ctx = TestContext::new().await;
    let auth_service = AuthServiceImpl::new(ctx.app_state.actor_repo.clone());

    // Mock OIDC token (stub validation for testing)
    let request = tonic::Request::new(AuthenticateRequest {
        oidc_token: "test_sub_123:Test User".to_string(),
    });

    let response = auth_service
        .authenticate(request)
        .await
        .expect("Authentication failed");

    let auth_response = response.into_inner();
    let actor = auth_response.actor.expect("Actor should be present");

    assert!(!actor.id.is_empty());
    assert_eq!(actor.kind, "user");
    assert_eq!(auth_response.display_name, "Test User");
}

#[tokio::test]
async fn test_authenticate_updates_existing_actor() {
    let ctx = TestContext::new().await;
    let auth_service = AuthServiceImpl::new(ctx.app_state.actor_repo.clone());

    // First authentication
    let request1 = tonic::Request::new(AuthenticateRequest {
        oidc_token: "test_sub_456:Original Name".to_string(),
    });

    let response1 = auth_service
        .authenticate(request1)
        .await
        .expect("First authentication failed");

    let actor1 = response1.into_inner().actor.unwrap();

    // Second authentication with updated display name
    let request2 = tonic::Request::new(AuthenticateRequest {
        oidc_token: "test_sub_456:Updated Name".to_string(),
    });

    let response2 = auth_service
        .authenticate(request2)
        .await
        .expect("Second authentication failed");

    let auth_response2 = response2.into_inner();
    let actor2 = auth_response2.actor.unwrap();

    // Should be the same actor ID
    assert_eq!(actor1.id, actor2.id);
    // But display name should be updated
    assert_eq!(auth_response2.display_name, "Updated Name");
}

#[tokio::test]
async fn test_authenticate_empty_token() {
    let ctx = TestContext::new().await;
    let auth_service = AuthServiceImpl::new(ctx.app_state.actor_repo.clone());

    let request = tonic::Request::new(AuthenticateRequest {
        oidc_token: "".to_string(),
    });

    let result = auth_service.authenticate(request).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
}

#[tokio::test]
async fn test_authenticate_simple_token() {
    let ctx = TestContext::new().await;
    let auth_service = AuthServiceImpl::new(ctx.app_state.actor_repo.clone());

    // Simple token without colon separator
    let request = tonic::Request::new(AuthenticateRequest {
        oidc_token: "simple_token".to_string(),
    });

    let response = auth_service
        .authenticate(request)
        .await
        .expect("Authentication failed");

    let auth_response = response.into_inner();
    let actor = auth_response.actor.unwrap();

    assert!(!actor.id.is_empty());
    // Display name should default to token value
    assert_eq!(auth_response.display_name, "simple_token");
}

#[tokio::test]
async fn test_multiple_actors_authentication() {
    let ctx = TestContext::new().await;
    let auth_service = AuthServiceImpl::new(ctx.app_state.actor_repo.clone());

    // Authenticate multiple different actors
    let actors = vec![
        ("user1:User One", "User One"),
        ("user2:User Two", "User Two"),
        ("user3:User Three", "User Three"),
    ];

    let mut actor_ids = std::collections::HashSet::new();

    for (token, expected_name) in actors {
        let request = tonic::Request::new(AuthenticateRequest {
            oidc_token: token.to_string(),
        });

        let response = auth_service
            .authenticate(request)
            .await
            .expect("Authentication failed");

        let auth_response = response.into_inner();
        let actor = auth_response.actor.unwrap();

        assert!(!actor.id.is_empty());
        assert_eq!(auth_response.display_name, expected_name);

        // Each actor should have unique ID
        assert!(actor_ids.insert(actor.id));
    }

    assert_eq!(actor_ids.len(), 3);
}

#[tokio::test]
async fn test_concurrent_authentications() {
    let ctx = TestContext::new().await;
    let auth_service = AuthServiceImpl::new(ctx.app_state.actor_repo.clone());

    use futures::future::join_all;

    // Authenticate 10 different users concurrently
    let mut tasks = vec![];
    for i in 0..10 {
        let service = auth_service.clone();
        tasks.push(tokio::spawn(async move {
            let request = tonic::Request::new(AuthenticateRequest {
                oidc_token: format!("concurrent_user_{}:User {}", i, i),
            });

            service.authenticate(request).await
        }));
    }

    let results = join_all(tasks).await;

    // All should succeed
    let mut actor_ids = std::collections::HashSet::new();
    for result in results {
        let response = result.unwrap().expect("Authentication failed");
        let actor = response.into_inner().actor.unwrap();
        actor_ids.insert(actor.id);
    }

    // Should have 10 unique actors
    assert_eq!(actor_ids.len(), 10);
}
