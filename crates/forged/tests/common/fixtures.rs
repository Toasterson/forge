use crate::common::TestContext;
use forged::entities::{actor, component, gate};

pub struct TestFixtures;

impl TestFixtures {
    /// Create test actor
    pub async fn actor(ctx: &TestContext, name: &str) -> actor::Model {
        ctx.app_state
            .actor_repo
            .create_or_update_from_oidc(format!("test_sub_{}", name), format!("Test {}", name))
            .await
            .expect("Failed to create test actor")
    }

    /// Create test gate with owner
    pub async fn gate(ctx: &TestContext, owner: &actor::Model, name: &str) -> gate::Model {
        let gate_kdl = format!(
            "name \"{name}\"\nversion \"0.5.11\"\nbranch \"2024.0.0\"\npublisher \"test.example.com\""
        );
        ctx.app_state
            .gate_manager
            .create_gate(&owner.id, name.to_string(), gate_kdl)
            .await
            .expect("Failed to create test gate")
    }

    /// Create test component
    pub async fn component(
        ctx: &TestContext,
        actor: &actor::Model,
        gate: &gate::Model,
        name: &str,
    ) -> component::Model {
        ctx.app_state
            .component_manager
            .create_component(
                &actor.id,
                &gate.id,
                name.to_string(),
                format!("name \"{name}\"\nsummary \"Test component\"\nversion \"1.0.0\""),
            )
            .await
            .expect("Failed to create test component")
    }

    /// Create test blob (returns hash)
    pub async fn blob(ctx: &TestContext, data: &[u8]) -> String {
        use forged::repositories::ApplicationBlobType;
        let (hash, _fid) = ctx
            .app_state
            .blob_repo
            .store_blob(data, ApplicationBlobType::SourceArchive)
            .await
            .expect("Failed to store test blob");
        hash
    }
}

/// Test data builders
pub mod builders {
    use uuid::Uuid;

    pub struct ActorBuilder {
        oidc_sub: String,
        display_name: String,
    }

    impl ActorBuilder {
        pub fn new() -> Self {
            Self {
                oidc_sub: format!("test_sub_{}", Uuid::new_v4().simple()),
                display_name: "Test User".to_string(),
            }
        }

        pub fn with_oidc_sub(mut self, sub: String) -> Self {
            self.oidc_sub = sub;
            self
        }

        pub fn with_display_name(mut self, name: String) -> Self {
            self.display_name = name;
            self
        }

        pub fn build(self) -> (String, String) {
            (self.oidc_sub, self.display_name)
        }
    }

    impl Default for ActorBuilder {
        fn default() -> Self {
            Self::new()
        }
    }
}
