use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(ServerMember::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ServerMember::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ServerMember::ActorId)
                            .string()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(ServerMember::Roles)
                            .json()
                            .not_null()
                            .default(Expr::val("[]")),
                    )
                    .col(
                        ColumnDef::new(ServerMember::Permissions)
                            .json()
                            .not_null()
                            .default(Expr::val("[]")),
                    )
                    .col(
                        ColumnDef::new(ServerMember::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(ServerMember::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_server_member_actor")
                            .from(ServerMember::Table, ServerMember::ActorId)
                            .to(Actor::Table, Actor::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on actor_id for lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_server_member_actor_id")
                    .table(ServerMember::Table)
                    .col(ServerMember::ActorId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ServerMember::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum ServerMember {
    Table,
    Id,
    ActorId,
    Roles,
    Permissions,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Actor {
    Table,
    Id,
}
