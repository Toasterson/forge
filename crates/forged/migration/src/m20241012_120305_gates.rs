use loco_rs::schema::table_auto_tz;
use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table_auto_tz(Gates::Table)
                    .col(pk_auto(Gates::Id))
                    .col(uuid_uniq(Gates::Pid))
                    .col(string_uniq(Gates::Name))
                    .col(string(Gates::Version))
                    .col(string(Gates::Branch))
                    .col(json_binary_null(Gates::Transforms))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Gates::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum Gates {
    Table,
    Id,
    Pid,
    Name,
    Version,
    Branch,
    Transforms,
    
}


