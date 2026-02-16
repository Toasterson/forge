use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. Add email and confirmed columns to actor table
        manager
            .alter_table(
                Table::alter()
                    .table(Actor::Table)
                    .add_column(ColumnDef::new(Actor::Email).string().null())
                    .add_column(
                        ColumnDef::new(Actor::Confirmed)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .add_column(ColumnDef::new(Actor::ConfirmationChallenge).string().null())
                    .to_owned(),
            )
            .await?;

        // 2. Create actor_key table
        manager
            .create_table(
                Table::create()
                    .table(ActorKey::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ActorKey::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(ActorKey::ActorId).string().not_null())
                    .col(ColumnDef::new(ActorKey::KeyId).string().not_null())
                    .col(ColumnDef::new(ActorKey::Algorithm).string().not_null())
                    .col(ColumnDef::new(ActorKey::PublicKey).text().not_null())
                    .col(
                        ColumnDef::new(ActorKey::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_actor_key_actor")
                            .from(ActorKey::Table, ActorKey::ActorId)
                            .to(Actor::Table, Actor::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on actor_id for listing keys by actor
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_actor_key_actor_id")
                    .table(ActorKey::Table)
                    .col(ActorKey::ActorId)
                    .to_owned(),
            )
            .await?;

        // Unique index on (actor_id, key_id) to prevent duplicate labels
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_actor_key_unique")
                    .table(ActorKey::Table)
                    .col(ActorKey::ActorId)
                    .col(ActorKey::KeyId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(ActorKey::Table).to_owned())
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Actor::Table)
                    .drop_column(Actor::Email)
                    .drop_column(Actor::Confirmed)
                    .drop_column(Actor::ConfirmationChallenge)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Actor {
    Table,
    Id,
    Email,
    Confirmed,
    ConfirmationChallenge,
}

#[derive(DeriveIden)]
enum ActorKey {
    Table,
    Id,
    ActorId,
    KeyId,
    Algorithm,
    PublicKey,
    CreatedAt,
}
