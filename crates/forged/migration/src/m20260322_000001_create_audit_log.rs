use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AuditLog::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AuditLog::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AuditLog::ActorId).string().not_null())
                    .col(ColumnDef::new(AuditLog::Action).string().not_null())
                    .col(ColumnDef::new(AuditLog::ResourceType).string().not_null())
                    .col(ColumnDef::new(AuditLog::ResourceId).string().not_null())
                    .col(ColumnDef::new(AuditLog::Outcome).string().not_null())
                    .col(ColumnDef::new(AuditLog::Details).text().null())
                    .col(
                        ColumnDef::new(AuditLog::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_audit_log_actor_id")
                    .table(AuditLog::Table)
                    .col(AuditLog::ActorId)
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_audit_log_created_at")
                    .table(AuditLog::Table)
                    .col(AuditLog::CreatedAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AuditLog::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum AuditLog {
    Table,
    Id,
    ActorId,
    Action,
    ResourceType,
    ResourceId,
    Outcome,
    Details,
    CreatedAt,
}
