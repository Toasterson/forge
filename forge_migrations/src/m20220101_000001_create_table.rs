use sea_orm_migration::prelude::*;
use crate::extension::postgres::TypeCreateStatement;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Publisher::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Publisher::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Publisher::Name).string().not_null())
                    .col(
                        ColumnDef::new(Publisher::PackageRepositoryId)
                            .uuid()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Gate::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Gate::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Gate::Name).string().not_null())
                    .col(ColumnDef::new(Gate::Version).string().not_null())
                    .col(ColumnDef::new(Gate::Ref).string().not_null())
                    .col(ColumnDef::new(Gate::Branch).string().not_null())
                    .col(ColumnDef::new(Gate::PublisherId).uuid().not_null())
                    .col(ColumnDef::new(Gate::Transforms).json().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("gate_publisher_id")
                    .from_tbl(Gate::Table)
                    .from_col(Gate::PublisherId)
                    .to_tbl(Publisher::Table)
                    .to_col(Publisher::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Component::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Component::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Component::Name).string().not_null())
                    .col(ColumnDef::new(Component::Version).string().not_null())
                    .col(ColumnDef::new(Component::GateId).uuid().not_null())
                    .col(ColumnDef::new(Component::Recipe).json().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("component_gate_id")
                    .from_tbl(Component::Table)
                    .from_col(Component::GateId)
                    .to_tbl(Gate::Table)
                    .to_col(Gate::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Package::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(Package::Id).uuid().not_null().primary_key())
                    .col(ColumnDef::new(Package::Name).string().not_null())
                    .col(ColumnDef::new(Package::Version).string().not_null())
                    .col(ColumnDef::new(Package::ComponentId).uuid().not_null())
                    .col(ColumnDef::new(Package::Manifest).json().not_null())
                    .col(ColumnDef::new(Package::ManifestFormat).string().not_null())
                    .col(ColumnDef::new(Package::ManifestSha256).string().not_null())
                    .col(ColumnDef::new(Package::Contents).json().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("package_component_id")
                    .from_tbl(Package::Table)
                    .from_col(Package::ComponentId)
                    .to_tbl(Component::Table)
                    .to_col(Component::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PackageRepository::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PackageRepository::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(PackageRepository::Name).string().not_null())
                    .col(ColumnDef::new(PackageRepository::Url).string().not_null())
                    .col(
                        ColumnDef::new(PackageRepository::PublicKey)
                            .string()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("publisher_package_repository_id")
                    .from_tbl(Publisher::Table)
                    .from_col(Publisher::PackageRepositoryId)
                    .to_tbl(PackageRepository::Table)
                    .to_col(PackageRepository::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Blob::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Blob::Sha256)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Blob::Url).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager.create_type(TypeCreateStatement::new().as_enum(SourceRepoKind::SourceRepoKind)
            .values([SourceRepoKind::Recipes, SourceRepoKind::Upstream]).to_owned()).await?;

        manager.create_type(TypeCreateStatement::new().as_enum(RecipeKind::RecipeKind)
            .values([RecipeKind::Forge, RecipeKind::OpenIndianaUserland]).to_owned()).await?;

        manager
            .create_table(
                Table::create()
                    .table(SourceRepo::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SourceRepo::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(SourceRepo::Name).string().not_null())
                    .col(ColumnDef::new(SourceRepo::Url).string().not_null())
                    .col(ColumnDef::new(SourceRepo::RepoKind).enumeration(SourceRepoKind::SourceRepoKind,
                      [SourceRepoKind::Recipes, SourceRepoKind::Upstream]).not_null())
                    .col(ColumnDef::new(SourceRepo::RecipeKind).enumeration(RecipeKind::RecipeKind,
                            [RecipeKind::Forge, RecipeKind::OpenIndianaUserland]).not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SourceMergeRequest::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SourceMergeRequest::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SourceMergeRequest::Number)
                            .integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SourceMergeRequest::Url).string().not_null())
                    .col(
                        ColumnDef::new(SourceMergeRequest::Repository)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SourceMergeRequest::State)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SourceMergeRequest::APIKind)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SourceMergeRequest::TargetRef)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(SourceMergeRequest::MergeRequestRef)
                            .json_binary()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SourceRepoPush::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SourceRepoPush::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SourceRepoPush::RepositoryId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SourceRepoPush::Ref).string().not_null())
                    .col(ColumnDef::new(SourceRepoPush::Sha256).string().not_null())
                    .col(ColumnDef::new(SourceRepoPush::Url).string().not_null())
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(SourceToGateRecord::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SourceToGateRecord::SourceRepoId)
                            .uuid()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SourceToGateRecord::GateId).uuid().not_null())
                    .primary_key(
                        Index::create()
                            .name("source_to_gate_record_pk")
                            .col(SourceToGateRecord::SourceRepoId)
                            .col(SourceToGateRecord::GateId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PushToComponentRecord::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PushToComponentRecord::PushId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PushToComponentRecord::ComponentId)
                            .uuid()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .name("push_to_component_record_pk")
                            .col(PushToComponentRecord::PushId)
                            .col(PushToComponentRecord::ComponentId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(MergeRequestToComponentRecord::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(MergeRequestToComponentRecord::MergeRequestId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(MergeRequestToComponentRecord::ComponentId)
                            .uuid()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .name("merge_request_to_component_record_pk")
                            .col(MergeRequestToComponentRecord::MergeRequestId)
                            .col(MergeRequestToComponentRecord::ComponentId),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("push_to_component_record_push_id")
                    .from_tbl(PushToComponentRecord::Table)
                    .from_col(PushToComponentRecord::PushId)
                    .to_tbl(SourceRepoPush::Table)
                    .to_col(SourceRepoPush::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("push_to_component_record_component_id")
                    .from_tbl(PushToComponentRecord::Table)
                    .from_col(PushToComponentRecord::ComponentId)
                    .to_tbl(Component::Table)
                    .to_col(Component::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("source_to_gate_record_source_repo_id")
                    .from_tbl(SourceToGateRecord::Table)
                    .from_col(SourceToGateRecord::SourceRepoId)
                    .to_tbl(SourceRepo::Table)
                    .to_col(SourceRepo::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager
            .create_foreign_key(
                ForeignKey::create()
                    .name("source_to_gate_record_gate_id")
                    .from_tbl(SourceToGateRecord::Table)
                    .from_col(SourceToGateRecord::GateId)
                    .to_tbl(Gate::Table)
                    .to_col(Gate::Id)
                    .on_delete(ForeignKeyAction::Cascade)
                    .to_owned(),
            )
            .await?;

        manager.create_table(
            Table::create().table(Job::Table)
                .if_not_exists()
                .col(ColumnDef::new(Job::Id).uuid().not_null().primary_key(), )
                .col(ColumnDef::new(Job::Patch).string().null(), )
                .col(ColumnDef::new(Job::MergeRequestRef).json_binary().not_null(), )
                .col(ColumnDef::new(Job::TargetRef).json_binary().not_null(), )
                .col(ColumnDef::new(Job::Repository).string().not_null())
                .col(ColumnDef::new(Job::ConfRef).json_binary().null())
                .col(ColumnDef::new(Job::Tags).array(ColumnType::Text).null())
                .col(ColumnDef::new(Job::JobType).string().null())
                .col(ColumnDef::new(Job::PackageRepoId).uuid().null())
                .col(ColumnDef::new(Job::SourceRepoId).uuid().not_null())
                .foreign_key(ForeignKey::create().from(Job::Table, Job::PackageRepoId).to(PackageRepository::Table, PackageRepository::Id))
                .foreign_key(ForeignKey::create().from(Job::Table, Job::SourceRepoId).to(SourceRepo::Table, SourceRepo::Id))
                .to_owned()
        ).await?;

        manager.create_table(
            Table::create().table(JobToComponentRecord::Table)
                .if_not_exists()
                .col(ColumnDef::new(JobToComponentRecord::JobId).uuid().not_null())
                .col(ColumnDef::new(JobToComponentRecord::ComponentId).uuid().not_null()).primary_key(
                Index::create()
                    .name("job_to_component_pk")
                    .col(JobToComponentRecord::JobId)
                    .col(JobToComponentRecord::ComponentId),
                )
                .to_owned()
        ).await?;

        manager.create_foreign_key(
            ForeignKey::create()
               .name("job_to_component_job_id")
               .from_tbl(JobToComponentRecord::Table)
                .from_col(JobToComponentRecord::JobId)
               .to_tbl(Job::Table)
               .to_col(Job::Id)
               .on_delete(ForeignKeyAction::Cascade)
               .to_owned()
        ).await?;

        manager.create_foreign_key(
            ForeignKey::create()
                .name("job_to_component_to_component_id")
                .from_tbl(JobToComponentRecord::Table)
                .from_col(JobToComponentRecord::ComponentId)
                .to_tbl(Component::Table)
                .to_col(Component::Id)
                .on_delete(ForeignKeyAction::Cascade)
                .to_owned()
        ).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Blob::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(PackageRepository::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Package::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Component::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Gate::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Publisher::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(SourceToGateRecord::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(PushToComponentRecord::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SourceRepoPush::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SourceMergeRequest::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(SourceRepo::Table).to_owned())
            .await?;
        manager.drop_table(Table::drop().table(Job::Table).to_owned()).await?;
        manager.drop_table(Table::drop().table(JobToComponentRecord::Table).to_owned()).await
    }
}

#[derive(DeriveIden)]
enum JobToComponentRecord {
    Table,
    JobId,
    ComponentId,
}

#[derive(DeriveIden)]
enum Job {
    Table,
    Id,
    SourceRepoId,
    PackageRepoId,
    Patch,
    MergeRequestRef,
    TargetRef,
    ConfRef,
    Repository,
    Tags,
    JobType,
}

#[derive(DeriveIden)]
enum SourceRepo {
    Table,
    Id,
    Name,
    Url,
    RepoKind,
    RecipeKind,
}

#[derive(DeriveIden)]
enum RecipeKind {
    RecipeKind,
    OpenIndianaUserland,
    Forge,
}

#[derive(DeriveIden)]
enum SourceRepoKind {
    SourceRepoKind,
    Recipes,
    Upstream,
}

#[derive(DeriveIden)]
enum SourceMergeRequest {
    Table,
    Id,
    Number,
    Url,
    Repository,
    State,
    APIKind,
    TargetRef,
    MergeRequestRef,
}

#[derive(DeriveIden)]
enum SourceRepoPush {
    Table,
    Id,
    RepositoryId,
    Ref,
    Sha256,
    Url,
}

#[derive(DeriveIden)]
enum SourceToGateRecord {
    Table,
    SourceRepoId,
    GateId,
}

#[derive(DeriveIden)]
enum PushToComponentRecord {
    Table,
    PushId,
    ComponentId,
}

#[derive(DeriveIden)]
enum MergeRequestToComponentRecord {
    Table,
    MergeRequestId,
    ComponentId,
}

#[derive(DeriveIden)]
enum Publisher {
    Table,
    Id,
    Name,
    PackageRepositoryId,
}

#[derive(DeriveIden)]
enum Gate {
    Table,
    Id,
    Name,
    Ref,
    Version,
    Branch,
    PublisherId,
    Transforms,
}

#[derive(DeriveIden)]
enum Component {
    Table,
    Id,
    Name,
    Version,
    GateId,
    Recipe,
}

#[derive(DeriveIden)]
enum Package {
    Table,
    Id,
    Name,
    Version,
    ComponentId,
    Manifest,
    ManifestFormat,
    ManifestSha256,
    Contents,
}

#[derive(DeriveIden)]
enum PackageRepository {
    Table,
    Id,
    Name,
    Url,
    PublicKey,
}

#[derive(DeriveIden)]
enum Blob {
    Table,
    Sha256,
    Url,
}
