use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 1. Create actors table (no dependencies)
        manager
            .create_table(
                Table::create()
                    .table(Actor::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Actor::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Actor::Kind).string().not_null())
                    .col(ColumnDef::new(Actor::OidcSub).string())
                    .col(ColumnDef::new(Actor::DisplayName).string().not_null())
                    .col(
                        ColumnDef::new(Actor::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Actor::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on oidc_sub (where not null)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_actor_oidc_sub")
                    .table(Actor::Table)
                    .col(Actor::OidcSub)
                    .unique()
                    .nulls_not_distinct()
                    .to_owned(),
            )
            .await?;

        // 2. Create blob_metadata table (no dependencies)
        manager
            .create_table(
                Table::create()
                    .table(BlobMetadata::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BlobMetadata::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BlobMetadata::Hash).string().not_null())
                    .col(ColumnDef::new(BlobMetadata::BlobType).string().not_null())
                    .col(ColumnDef::new(BlobMetadata::Namespace).string().not_null())
                    .col(ColumnDef::new(BlobMetadata::Fid).string().not_null())
                    .col(
                        ColumnDef::new(BlobMetadata::SizeBytes)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BlobMetadata::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on (hash, blob_type, namespace)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_blob_metadata_unique")
                    .table(BlobMetadata::Table)
                    .col(BlobMetadata::Hash)
                    .col(BlobMetadata::BlobType)
                    .col(BlobMetadata::Namespace)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create index on hash for lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_blob_metadata_hash")
                    .table(BlobMetadata::Table)
                    .col(BlobMetadata::Hash)
                    .to_owned(),
            )
            .await?;

        // 3. Create gates table (depends on actors)
        manager
            .create_table(
                Table::create()
                    .table(Gate::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Gate::Id).string().not_null().primary_key())
                    .col(ColumnDef::new(Gate::Name).string().not_null())
                    .col(ColumnDef::new(Gate::GateKdl).text().not_null())
                    .col(ColumnDef::new(Gate::OwnerId).string().not_null())
                    .col(
                        ColumnDef::new(Gate::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Gate::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_gate_owner")
                            .from(Gate::Table, Gate::OwnerId)
                            .to(Actor::Table, Actor::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on gate name for searches
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_gate_name")
                    .table(Gate::Table)
                    .col(Gate::Name)
                    .to_owned(),
            )
            .await?;

        // Create index on owner_id
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_gate_owner_id")
                    .table(Gate::Table)
                    .col(Gate::OwnerId)
                    .to_owned(),
            )
            .await?;

        // 4. Create gate_members table (depends on gates and actors)
        manager
            .create_table(
                Table::create()
                    .table(GateMember::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GateMember::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(GateMember::GateId).string().not_null())
                    .col(ColumnDef::new(GateMember::ActorId).string().not_null())
                    .col(ColumnDef::new(GateMember::Roles).json().not_null())
                    .col(ColumnDef::new(GateMember::Permissions).json().not_null())
                    .col(
                        ColumnDef::new(GateMember::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(GateMember::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_gate_member_gate")
                            .from(GateMember::Table, GateMember::GateId)
                            .to(Gate::Table, Gate::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_gate_member_actor")
                            .from(GateMember::Table, GateMember::ActorId)
                            .to(Actor::Table, Actor::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create unique index on (gate_id, actor_id)
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_gate_member_unique")
                    .table(GateMember::Table)
                    .col(GateMember::GateId)
                    .col(GateMember::ActorId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // Create index on actor_id for reverse lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_gate_member_actor_id")
                    .table(GateMember::Table)
                    .col(GateMember::ActorId)
                    .to_owned(),
            )
            .await?;

        // 5. Create components table (depends on gates)
        manager
            .create_table(
                Table::create()
                    .table(Component::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Component::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Component::GateId).string().not_null())
                    .col(ColumnDef::new(Component::Name).string().not_null())
                    .col(ColumnDef::new(Component::RecipeKdl).text().not_null())
                    .col(
                        ColumnDef::new(Component::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Component::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_component_gate")
                            .from(Component::Table, Component::GateId)
                            .to(Gate::Table, Gate::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on gate_id for listing components by gate
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_component_gate_id")
                    .table(Component::Table)
                    .col(Component::GateId)
                    .to_owned(),
            )
            .await?;

        // Create index on component name for searches
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_component_name")
                    .table(Component::Table)
                    .col(Component::Name)
                    .to_owned(),
            )
            .await?;

        // 6. Create source_archives table (depends on components)
        manager
            .create_table(
                Table::create()
                    .table(SourceArchive::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SourceArchive::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SourceArchive::ComponentId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SourceArchive::Filename).string().not_null())
                    .col(ColumnDef::new(SourceArchive::Url).string())
                    .col(ColumnDef::new(SourceArchive::BlobHash).string().not_null())
                    .col(
                        ColumnDef::new(SourceArchive::SizeBytes)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SourceArchive::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_source_archive_component")
                            .from(SourceArchive::Table, SourceArchive::ComponentId)
                            .to(Component::Table, Component::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on component_id for listing archives by component
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_source_archive_component_id")
                    .table(SourceArchive::Table)
                    .col(SourceArchive::ComponentId)
                    .to_owned(),
            )
            .await?;

        // Create index on blob_hash for blob ownership lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_source_archive_blob_hash")
                    .table(SourceArchive::Table)
                    .col(SourceArchive::BlobHash)
                    .to_owned(),
            )
            .await?;

        // 7. Create component_files table (depends on components)
        manager
            .create_table(
                Table::create()
                    .table(ComponentFile::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(ComponentFile::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(ComponentFile::ComponentId)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(ComponentFile::Kind).string().not_null())
                    .col(ColumnDef::new(ComponentFile::Name).string().not_null())
                    .col(ColumnDef::new(ComponentFile::RelPath).string().not_null())
                    .col(ColumnDef::new(ComponentFile::BlobHash).string().not_null())
                    .col(
                        ColumnDef::new(ComponentFile::SizeBytes)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(ComponentFile::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_component_file_component")
                            .from(ComponentFile::Table, ComponentFile::ComponentId)
                            .to(Component::Table, Component::Id)
                            .on_delete(ForeignKeyAction::Cascade)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on component_id for listing files by component
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_component_file_component_id")
                    .table(ComponentFile::Table)
                    .col(ComponentFile::ComponentId)
                    .to_owned(),
            )
            .await?;

        // Create index on (component_id, kind) for filtering by file type
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_component_file_component_kind")
                    .table(ComponentFile::Table)
                    .col(ComponentFile::ComponentId)
                    .col(ComponentFile::Kind)
                    .to_owned(),
            )
            .await?;

        // Create index on blob_hash for blob ownership lookups
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_component_file_blob_hash")
                    .table(ComponentFile::Table)
                    .col(ComponentFile::BlobHash)
                    .to_owned(),
            )
            .await?;

        // 8. Create operations table (stub for distributed sync - deferred)
        manager
            .create_table(
                Table::create()
                    .table(Operation::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Operation::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Operation::OpType).string().not_null())
                    .col(ColumnDef::new(Operation::EntityType).string().not_null())
                    .col(ColumnDef::new(Operation::EntityId).string().not_null())
                    .col(ColumnDef::new(Operation::ActorId).string().not_null())
                    .col(ColumnDef::new(Operation::Payload).json().not_null())
                    .col(
                        ColumnDef::new(Operation::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_operation_actor")
                            .from(Operation::Table, Operation::ActorId)
                            .to(Actor::Table, Actor::Id)
                            .on_delete(ForeignKeyAction::Restrict)
                            .on_update(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Create index on created_at for operation log ordering
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_operation_created_at")
                    .table(Operation::Table)
                    .col(Operation::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // Create index on (entity_type, entity_id) for entity history
        manager
            .create_index(
                Index::create()
                    .if_not_exists()
                    .name("idx_operation_entity")
                    .table(Operation::Table)
                    .col(Operation::EntityType)
                    .col(Operation::EntityId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Drop tables in reverse order of creation to respect foreign keys
        manager
            .drop_table(Table::drop().table(Operation::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(ComponentFile::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SourceArchive::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Component::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(GateMember::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Gate::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BlobMetadata::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Actor::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Actor {
    Table,
    Id,
    Kind,
    OidcSub,
    DisplayName,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum BlobMetadata {
    Table,
    Id,
    Hash,
    BlobType,
    Namespace,
    Fid,
    SizeBytes,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Gate {
    Table,
    Id,
    Name,
    GateKdl,
    OwnerId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum GateMember {
    Table,
    Id,
    GateId,
    ActorId,
    Roles,
    Permissions,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Component {
    Table,
    Id,
    GateId,
    Name,
    RecipeKdl,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum SourceArchive {
    Table,
    Id,
    ComponentId,
    Filename,
    Url,
    BlobHash,
    SizeBytes,
    CreatedAt,
}

#[derive(DeriveIden)]
enum ComponentFile {
    Table,
    Id,
    ComponentId,
    Kind,
    Name,
    RelPath,
    BlobHash,
    SizeBytes,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Operation {
    Table,
    Id,
    OpType,
    EntityType,
    EntityId,
    ActorId,
    Payload,
    CreatedAt,
}
