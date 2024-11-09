use loco_rs::schema::table_auto_tz;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table_auto_tz(Archives::Table)
                    .col(pk_auto(Archives::Id))
                    .col(string(Archives::Name))
                    .col(integer(Archives::ComponentId))
                    .col(string(Archives::Sha256))
                    .col(string(Archives::Sha512))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-archives-components")
                            .from(Archives::Table, Archives::ComponentId)
                            .to(Components::Table, Components::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Archives::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Archives {
    Table,
    Id,
    Name,
    ComponentId,
    Sha256,
    Sha512,
}

#[derive(DeriveIden)]
enum Components {
    Table,
    Id,
}
