use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "build_job")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String, // UUID
    pub component_id: String,
    pub gate_id: String,
    pub actor_id: String,
    /// Correlates to solstice-ci JobRequest.request_id
    pub request_id: String,
    /// queued, running, success, failed, cancelled
    pub status: String,
    pub exit_code: Option<i32>,
    pub summary: Option<String>,
    pub build_log_url: Option<String>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub completed_at: Option<DateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::component::Entity",
        from = "Column::ComponentId",
        to = "super::component::Column::Id"
    )]
    Component,
    #[sea_orm(
        belongs_to = "super::gate::Entity",
        from = "Column::GateId",
        to = "super::gate::Column::Id"
    )]
    Gate,
    #[sea_orm(
        belongs_to = "super::actor::Entity",
        from = "Column::ActorId",
        to = "super::actor::Column::Id"
    )]
    Actor,
}

impl Related<super::component::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Component.def()
    }
}

impl Related<super::gate::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Gate.def()
    }
}

impl Related<super::actor::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Actor.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
