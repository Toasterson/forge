use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BuildJob::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BuildJob::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BuildJob::ComponentId).string().not_null())
                    .col(ColumnDef::new(BuildJob::GateId).string().not_null())
                    .col(ColumnDef::new(BuildJob::ActorId).string().not_null())
                    .col(ColumnDef::new(BuildJob::RequestId).string().not_null())
                    .col(
                        ColumnDef::new(BuildJob::Status)
                            .string()
                            .not_null()
                            .default("queued"),
                    )
                    .col(ColumnDef::new(BuildJob::ExitCode).integer().null())
                    .col(ColumnDef::new(BuildJob::Summary).text().null())
                    .col(ColumnDef::new(BuildJob::BuildLogUrl).text().null())
                    .col(
                        ColumnDef::new(BuildJob::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(BuildJob::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(BuildJob::CompletedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BuildJob::Table, BuildJob::ComponentId)
                            .to(Component::Table, Component::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BuildJob::Table, BuildJob::GateId)
                            .to(Gate::Table, Gate::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(BuildJob::Table, BuildJob::ActorId)
                            .to(Actor::Table, Actor::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Index on component_id for listing builds per component
        manager
            .create_index(
                Index::create()
                    .name("idx_build_job_component_id")
                    .table(BuildJob::Table)
                    .col(BuildJob::ComponentId)
                    .to_owned(),
            )
            .await?;

        // Unique index on request_id for correlation with solstice-ci
        manager
            .create_index(
                Index::create()
                    .name("idx_build_job_request_id")
                    .table(BuildJob::Table)
                    .col(BuildJob::RequestId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Index on status for querying active builds
        manager
            .create_index(
                Index::create()
                    .name("idx_build_job_status")
                    .table(BuildJob::Table)
                    .col(BuildJob::Status)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BuildJob::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum BuildJob {
    Table,
    Id,
    ComponentId,
    GateId,
    ActorId,
    RequestId,
    Status,
    ExitCode,
    Summary,
    BuildLogUrl,
    CreatedAt,
    UpdatedAt,
    CompletedAt,
}

#[derive(DeriveIden)]
enum Component {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Gate {
    Table,
    Id,
}

#[derive(DeriveIden)]
enum Actor {
    Table,
    Id,
}
