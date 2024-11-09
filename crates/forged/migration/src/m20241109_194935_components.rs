use loco_rs::schema::table_auto_tz;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table_auto_tz(Components::Table)
                    .col(pk_auto(Components::Id))
                    .col(string(Components::Name))
                    .col(string(Components::Version))
                    .col(string(Components::Revision))
                    .col(string_null(Components::AnityaId))
                    .col(string_null(Components::RepologyId))
                    .col(string_null(Components::ProjectUrl))
                    .col(integer(Components::GateId))
                    .col(json_binary(Components::Recipe))
                    .col(json_binary(Components::Patches))
                    .col(json_binary(Components::Scripts))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-components-gates")
                            .from(Components::Table, Components::GateId)
                            .to(Gates::Table, Gates::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Components::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Components {
    Table,
    Id,
    Name,
    Version,
    Revision,
    AnityaId,
    RepologyId,
    ProjectUrl,
    GateId,
    Recipe,
    Patches,
    Scripts,
}

#[derive(DeriveIden)]
enum Gates {
    Table,
    Id,
}
